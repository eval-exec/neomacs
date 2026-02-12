//! wgpu GPU-accelerated scene renderer.

use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::core::face::{BoxType, Face, FaceAttributes};
use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer, StipplePattern};
use crate::core::scene::{CursorStyle, Scene};
use crate::core::types::{AnimatedCursor, Color, Rect};

use super::glyph_atlas::{GlyphKey, WgpuGlyphAtlas};
use super::image_cache::ImageCache;
#[cfg(feature = "video")]
use super::video_cache::VideoCache;
#[cfg(feature = "wpe-webkit")]
use super::webkit_cache::WgpuWebKitCache;
use super::vertex::{GlyphVertex, RectVertex, RoundedRectVertex, Uniforms};

mod blur;
mod media;
mod effects_state;
mod glyphs;
mod transitions;
mod overlays;
mod cursor_effects;
mod effect_common;
mod window_effects;
mod pattern_effects;

/// GPU-accelerated renderer using wgpu.
pub struct WgpuRenderer {
    pub(crate) device: Arc<wgpu::Device>,
    pub(crate) queue: Arc<wgpu::Queue>,
    pub(super) surface: Option<wgpu::Surface<'static>>,
    pub(super) surface_config: Option<wgpu::SurfaceConfiguration>,
    pub(super) surface_format: wgpu::TextureFormat,
    pub(super) rect_pipeline: wgpu::RenderPipeline,
    pub(super) rounded_rect_pipeline: wgpu::RenderPipeline,
    pub(super) corner_mask_pipeline: wgpu::RenderPipeline,
    pub(super) glyph_pipeline: wgpu::RenderPipeline,
    pub(super) image_pipeline: wgpu::RenderPipeline,
    pub(super) opaque_image_pipeline: wgpu::RenderPipeline,
    pub(super) blur_pipeline: wgpu::RenderPipeline,
    pub(super) blur_uniform_bind_group_layout: wgpu::BindGroupLayout,
    pub(super) blur_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub(super) blur_sampler: wgpu::Sampler,
    /// Intermediate textures for ping-pong blur passes
    pub(super) blur_texture_a: Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
    pub(super) blur_texture_b: Option<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
    pub(super) glyph_bind_group_layout: wgpu::BindGroupLayout,
    pub(super) uniform_buffer: wgpu::Buffer,
    pub(super) uniform_bind_group: wgpu::BindGroup,
    pub(super) image_cache: ImageCache,
    #[cfg(feature = "video")]
    pub(super) video_cache: VideoCache,
    #[cfg(feature = "wpe-webkit")]
    pub(super) webkit_cache: WgpuWebKitCache,
    pub(super) width: u32,
    pub(super) height: u32,
    /// Display scale factor (physical pixels / logical pixels)
    pub(super) scale_factor: f32,

    // All visual effect configurations
    pub effects: crate::effect_config::EffectsConfig,
    /// Per-window dim opacity for smooth fade transitions
    pub(super) per_window_dim: std::collections::HashMap<i64, f32>,
    /// Last dim update time for smooth interpolation
    pub(super) last_dim_tick: std::time::Instant,
    /// Flag: renderer needs continuous redraws (e.g. dim fade in progress)
    pub needs_continuous_redraw: bool,
    /// Start time for pulse phase calculation
    pub(super) cursor_pulse_start: std::time::Instant,
    /// Ripple duration in seconds
    pub(super) typing_ripple_duration: f32,
    /// Active ripples: (center_x, center_y, spawn_instant)
    pub(super) active_ripples: Vec<(f32, f32, std::time::Instant)>,
    /// Pulse start time
    pub(super) search_pulse_start: std::time::Instant,
    pub(super) active_line_anims: Vec<LineAnimEntry>,
    pub(super) cursor_color_cycle_start: std::time::Instant,
    /// Active window switch fades: (window_id, bounds, started, duration, intensity)
    pub(super) active_window_fades: Vec<WindowFadeEntry>,
    pub(super) border_transition_duration: std::time::Duration,
    /// Per-window border transition state: (window_id, is_becoming_active, start_time)
    pub(super) border_transitions: Vec<(i64, bool, std::time::Instant)>,
    /// Previous selected window for border transition detection
    pub(super) prev_border_selected: i64,
    pub(super) cursor_trail_fade_duration: std::time::Duration,
    pub(super) cursor_trail_positions: Vec<(f32, f32, f32, f32, std::time::Instant)>,
    pub(super) cursor_trail_last_pos: (f32, f32),
    pub(super) focus_ring_start: std::time::Instant,
    /// Idle screen dimming alpha (0.0 = no dim, >0 = overlay)
    pub(super) idle_dim_alpha: f32,
    pub(super) noise_grain_frame: u32,
    /// Previous breadcrumb text per window (window_id -> file_name)
    pub(super) prev_breadcrumb_text: std::collections::HashMap<i64, String>,
    /// Active title fades (window_id -> (old_text, new_text, start_time))
    pub(super) active_title_fades: Vec<TitleFadeEntry>,
    /// Per-window mode-line content hash for change detection
    pub(super) prev_mode_line_hashes: std::collections::HashMap<i64, u64>,
    /// Active mode-line transition fades
    pub(super) active_mode_line_fades: Vec<ModeLineFadeEntry>,
    /// Active text fade-in animations per window
    pub(super) active_text_fades: Vec<TextFadeEntry>,
    pub(super) scroll_line_spacing_duration_ms: u32,
    /// Active scroll line spacing animations: (window_id, bounds, direction, started)
    pub(super) active_scroll_spacings: Vec<ScrollSpacingEntry>,
    /// Timestamp of last cursor wake trigger
    pub(super) cursor_wake_started: Option<std::time::Instant>,
    pub(super) click_halos: Vec<ClickHaloEntry>,
    pub(super) edge_snaps: Vec<EdgeSnapEntry>,
    pub(super) cursor_magnetism_entries: Vec<(f32, f32, std::time::Instant)>, // x, y, time
    pub(super) cursor_comet_positions: Vec<(f32, f32, f32, f32, std::time::Instant)>, // x, y, w, h, time
    pub(super) cursor_particles: Vec<CursorParticle>,
    pub(super) cursor_particles_prev_pos: Option<(f32, f32)>,
    pub(super) typing_heatmap_entries: Vec<HeatMapEntry>,
    pub(super) typing_heatmap_prev_cursor: Option<(f32, f32)>,
    pub(super) scroll_velocity_fades: Vec<ScrollVelocityFadeEntry>,
    pub(super) resize_padding_started: Option<std::time::Instant>,
    pub(super) cursor_error_pulse_started: Option<std::time::Instant>,
    /// Active scroll momentum entries
    pub(super) active_scroll_momentums: Vec<ScrollMomentumEntry>,
    pub(super) matrix_rain_columns: Vec<MatrixColumn>,
    pub(super) cursor_ghost_entries: Vec<CursorGhostEntry>,
    pub(super) cursor_sonar_ping_entries: Vec<SonarPingEntry>,
    pub(super) lightning_bolt_last: std::time::Instant,
    pub(super) lightning_bolt_segments: Vec<(f32, f32, f32, f32)>, // (x1, y1, x2, y2)
    pub(super) lightning_bolt_age: f32,
    pub(super) cursor_pendulum_last_x: f32,
    pub(super) cursor_pendulum_last_y: f32,
    pub(super) cursor_pendulum_swing_start: Option<std::time::Instant>,
    pub(super) cursor_sparkle_burst_entries: Vec<SparkleBurstEntry>,
    pub(super) cursor_metronome_last_x: f32,
    pub(super) cursor_metronome_last_y: f32,
    pub(super) cursor_metronome_tick_start: Option<std::time::Instant>,
    pub(super) cursor_ripple_ring_start: Option<std::time::Instant>,
    pub(super) cursor_ripple_ring_last_x: f32,
    pub(super) cursor_ripple_ring_last_y: f32,
    pub(super) cursor_shockwave_start: Option<std::time::Instant>,
    pub(super) cursor_shockwave_last_x: f32,
    pub(super) cursor_shockwave_last_y: f32,
    pub(super) cursor_bubble_spawn_time: Option<std::time::Instant>,
    pub(super) cursor_bubble_last_x: f32,
    pub(super) cursor_bubble_last_y: f32,
    pub(super) cursor_firework_start: Option<std::time::Instant>,
    pub(super) cursor_firework_last_x: f32,
    pub(super) cursor_firework_last_y: f32,
    pub(super) cursor_lightning_start: Option<std::time::Instant>,
    pub(super) cursor_lightning_last_x: f32,
    pub(super) cursor_lightning_last_y: f32,
    pub(super) cursor_snowflake_start: Option<std::time::Instant>,
    pub(super) cursor_snowflake_last_x: f32,
    pub(super) cursor_snowflake_last_y: f32,
    pub(super) edge_glow_entries: Vec<EdgeGlowEntry>,
    pub(super) rain_drops: Vec<RainDrop>,
    pub(super) rain_last_spawn: std::time::Instant,
    pub(super) cursor_ripple_waves: Vec<RippleWaveEntry>,
    pub(super) aurora_start: std::time::Instant,
}

/// Entry for an active scroll momentum indicator
pub(super) struct ScrollMomentumEntry {
    pub(super) window_id: i64,
    pub(super) bounds: Rect,
    pub(super) direction: i32, // 1 = down, -1 = up
    pub(super) started: std::time::Instant,
    pub(super) duration: std::time::Duration,
}

/// Entry for matrix rain column
pub(super) struct MatrixColumn {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) speed: f32,
    pub(super) length: f32,
}

/// Entry for cursor ghost afterimage
pub(super) struct CursorGhostEntry {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) started: std::time::Instant,
}

/// Entry for cursor sonar ping
pub(super) struct SonarPingEntry {
    pub(super) cx: f32,
    pub(super) cy: f32,
    pub(super) started: std::time::Instant,
    pub(super) duration: std::time::Duration,
}

pub(super) struct SparkleBurstEntry {
    pub(super) cx: f32,
    pub(super) cy: f32,
    pub(super) started: std::time::Instant,
    /// Random seed for particle directions
    pub(super) seed: u32,
}

/// Entry for window edge glow (scroll boundary indicator)
pub(super) struct EdgeGlowEntry {
    pub(super) window_id: i64,
    pub(super) bounds: Rect,
    pub(super) at_top: bool,
    pub(super) started: std::time::Instant,
    pub(super) duration: std::time::Duration,
}

/// Entry for rain drop
pub(super) struct RainDrop {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) speed: f32,
    pub(super) length: f32,
    pub(super) opacity: f32,
}

/// Entry for cursor ripple wave
pub(super) struct RippleWaveEntry {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) started: std::time::Instant,
    pub(super) duration: std::time::Duration,
}

/// Entry for window edge snap indicator
/// Entry for cursor particle effect
pub(super) struct CursorParticle {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) vx: f32,
    pub(super) vy: f32,
    pub(super) started: std::time::Instant,
    pub(super) lifetime: std::time::Duration,
}

/// Entry for typing heat map (records where cursor was during edits)
pub(super) struct HeatMapEntry {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) started: std::time::Instant,
}

pub(super) struct EdgeSnapEntry {
    pub(super) bounds: Rect,
    pub(super) mode_line_height: f32,
    pub(super) at_top: bool,
    pub(super) at_bottom: bool,
    pub(super) started: std::time::Instant,
    pub(super) duration: std::time::Duration,
}

/// Entry for click halo effect
pub(super) struct ClickHaloEntry {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) started: std::time::Instant,
    pub(super) duration: std::time::Duration,
}

/// Entry for scroll velocity fade overlay
pub(super) struct ScrollVelocityFadeEntry {
    pub(super) window_id: i64,
    pub(super) bounds: Rect,
    /// Scroll delta magnitude (characters scrolled)
    pub(super) velocity: f32,
    pub(super) started: std::time::Instant,
    pub(super) duration: std::time::Duration,
}

/// Entry for an active window switch highlight fade
pub(super) struct WindowFadeEntry {
    pub(super) window_id: i64,
    pub(super) bounds: Rect,
    pub(super) started: std::time::Instant,
    pub(super) duration: std::time::Duration,
    pub(super) intensity: f32,
}

/// Entry for an active title/breadcrumb crossfade animation
pub(super) struct TitleFadeEntry {
    pub(super) window_id: i64,
    pub(super) bounds: Rect,
    pub(super) old_text: String,
    pub(super) new_text: String,
    pub(super) started: std::time::Instant,
    pub(super) duration: std::time::Duration,
}

/// Entry for an active line insertion/deletion animation
pub(super) struct LineAnimEntry {
    /// Window bounds where the animation is active
    pub(super) window_bounds: Rect,
    /// Y position below which glyphs are displaced
    pub(super) edit_y: f32,
    /// Initial Y offset (negative=insertion slide-down, positive=deletion slide-up)
    pub(super) initial_offset: f32,
    /// When the animation started
    pub(super) started: std::time::Instant,
    /// Duration of the animation
    pub(super) duration: std::time::Duration,
}

/// Entry for an active mode-line content transition
pub(super) struct ModeLineFadeEntry {
    pub(super) window_id: i64,
    /// Mode-line area (y, height) within the window
    pub(super) mode_line_y: f32,
    pub(super) mode_line_h: f32,
    pub(super) bounds_x: f32,
    pub(super) bounds_w: f32,
    pub(super) started: std::time::Instant,
    pub(super) duration: std::time::Duration,
}

/// Entry for an active text fade-in animation
pub(super) struct TextFadeEntry {
    pub(super) window_id: i64,
    pub(super) bounds: Rect,
    pub(super) started: std::time::Instant,
    pub(super) duration: std::time::Duration,
}

/// Entry for an active scroll line spacing animation
pub(super) struct ScrollSpacingEntry {
    pub(super) window_id: i64,
    pub(super) bounds: Rect,
    /// +1 = scroll down (content moves up), -1 = scroll up
    pub(super) direction: i32,
    pub(super) started: std::time::Instant,
    pub(super) duration: std::time::Duration,
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
        Self::create_renderer_internal(device, queue, None, Some(surface_format), width, height, scale_factor)
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
    ) -> Self {
        // Create uniform buffer with logical size so vertex positions from Emacs map correctly
        let logical_w = width as f32 / scale_factor;
        let logical_h = height as f32 / scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
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
                visibility: wgpu::ShaderStages::VERTEX,
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
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Determine the target format
        let target_format = surface_format
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);

        // Create rect pipeline
        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rect Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rect_shader,
                entry_point: Some("vs_main"),
                buffers: &[RectVertex::desc()],
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
            multiview: None,
            cache: None,
        });

        // Load rounded rect shader (SDF-based rounded borders)
        let rounded_rect_shader_source = include_str!("../shaders/rounded_rect.wgsl");
        let rounded_rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rounded Rect Shader"),
            source: wgpu::ShaderSource::Wgsl(rounded_rect_shader_source.into()),
        });

        let rounded_rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rounded Rect Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rounded_rect_shader,
                entry_point: Some("vs_main"),
                buffers: &[RoundedRectVertex::desc()],
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
            multiview: None,
            cache: None,
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
                buffers: &[RoundedRectVertex::desc()],
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
            multiview: None,
            cache: None,
        });

        // Load glyph shader
        let glyph_shader_source = include_str!("../shaders/glyph.wgsl");
        let glyph_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Glyph Shader"),
            source: wgpu::ShaderSource::Wgsl(glyph_shader_source.into()),
        });

        // Glyph bind group layout (for per-glyph texture)
        let glyph_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let glyph_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Glyph Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout, &glyph_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create glyph pipeline
        let glyph_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Glyph Pipeline"),
            layout: Some(&glyph_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &glyph_shader,
                entry_point: Some("vs_main"),
                buffers: &[GlyphVertex::desc()],
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
            multiview: None,
            cache: None,
        });

        // Create image cache (also creates its bind group layout)
        let image_cache = ImageCache::new(&device);

        // Create video cache
        #[cfg(feature = "video")]
        let mut video_cache = VideoCache::new();
        #[cfg(feature = "video")]
        video_cache.init_gpu(&device);

        // Create webkit cache
        #[cfg(feature = "wpe-webkit")]
        let webkit_cache = WgpuWebKitCache::new(&device);

        // Load image shader
        let image_shader_source = include_str!("../shaders/image.wgsl");
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Image Shader"),
            source: wgpu::ShaderSource::Wgsl(image_shader_source.into()),
        });

        // Image pipeline layout (uniform + image texture)
        let image_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Image Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout, image_cache.bind_group_layout()],
            push_constant_ranges: &[],
        });

        // Create image pipeline (similar to glyph but for RGBA textures)
        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Image Pipeline"),
            layout: Some(&image_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: Some("vs_main"),
                buffers: &[GlyphVertex::desc()], // Reuse glyph vertex format
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
            multiview: None,
            cache: None,
        });

        // Opaque image pipeline — for XRGB/BGRX DMA-BUF textures where alpha=0x00.
        // Uses fs_main_opaque which ignores texture alpha and uses vertex alpha instead.
        let opaque_image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Opaque Image Pipeline"),
            layout: Some(&image_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: Some("vs_main"),
                buffers: &[GlyphVertex::desc()],
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
            multiview: None,
            cache: None,
        });

        // === Blur pipeline ===
        let blur_shader_source = include_str!("../shaders/blur.wgsl");
        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Blur Shader"),
            source: wgpu::ShaderSource::Wgsl(blur_shader_source.into()),
        });

        // Blur texture bind group layout: texture + sampler (group 1)
        let blur_texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Blur Texture Bind Group Layout"),
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

        // Blur uniform bind group layout: blur params (group 2)
        let blur_uniform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Blur Uniform Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let blur_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Blur Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout, &blur_texture_bind_group_layout, &blur_uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Blur Pipeline"),
            layout: Some(&blur_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blur_shader,
                entry_point: Some("vs_main"),
                buffers: &[GlyphVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blur_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None, // No blending — blur overwrites entirely
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
            multiview: None,
            cache: None,
        });

        let blur_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Blur Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create surface_config from format if we have a surface
        let surface_config = if let Some(ref s) = surface {
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: target_format,
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
            rect_pipeline,
            rounded_rect_pipeline,
            corner_mask_pipeline,
            glyph_pipeline,
            image_pipeline,
            opaque_image_pipeline,
            blur_pipeline,
            blur_uniform_bind_group_layout,
            blur_texture_bind_group_layout,
            blur_sampler,
            blur_texture_a: None,
            blur_texture_b: None,
            glyph_bind_group_layout,
            uniform_buffer,
            uniform_bind_group,
            image_cache,
            #[cfg(feature = "video")]
            video_cache,
            #[cfg(feature = "wpe-webkit")]
            webkit_cache,
            width,
            height,
            scale_factor,
            effects: crate::effect_config::EffectsConfig::default(),
            per_window_dim: std::collections::HashMap::new(),
            last_dim_tick: std::time::Instant::now(),
            needs_continuous_redraw: false,
            cursor_pulse_start: std::time::Instant::now(),
            typing_ripple_duration: 0.3,
            active_ripples: Vec::new(),
            search_pulse_start: std::time::Instant::now(),
            active_line_anims: Vec::new(),
            cursor_color_cycle_start: std::time::Instant::now(),
            active_window_fades: Vec::new(),
            border_transition_duration: std::time::Duration::from_millis(200),
            border_transitions: Vec::new(),
            prev_border_selected: 0,
            cursor_trail_fade_duration: std::time::Duration::from_millis(300),
            cursor_trail_positions: Vec::new(),
            cursor_trail_last_pos: (0.0, 0.0),
            focus_ring_start: std::time::Instant::now(),
            idle_dim_alpha: 0.0,
            noise_grain_frame: 0,
            prev_breadcrumb_text: std::collections::HashMap::new(),
            active_title_fades: Vec::new(),
            prev_mode_line_hashes: std::collections::HashMap::new(),
            active_mode_line_fades: Vec::new(),
            active_text_fades: Vec::new(),
            scroll_line_spacing_duration_ms: 200,
            active_scroll_spacings: Vec::new(),
            cursor_wake_started: None,
            click_halos: Vec::new(),
            edge_snaps: Vec::new(),
            cursor_magnetism_entries: Vec::new(),
            cursor_comet_positions: Vec::new(),
            cursor_particles: Vec::new(),
            cursor_particles_prev_pos: None,
            typing_heatmap_entries: Vec::new(),
            typing_heatmap_prev_cursor: None,
            scroll_velocity_fades: Vec::new(),
            resize_padding_started: None,
            cursor_error_pulse_started: None,
            active_scroll_momentums: Vec::new(),
            matrix_rain_columns: Vec::new(),
            cursor_ghost_entries: Vec::new(),
            cursor_sonar_ping_entries: Vec::new(),
            lightning_bolt_last: std::time::Instant::now(),
            lightning_bolt_segments: Vec::new(),
            lightning_bolt_age: 0.0,
            cursor_pendulum_last_x: 0.0,
            cursor_pendulum_last_y: 0.0,
            cursor_pendulum_swing_start: None,
            cursor_sparkle_burst_entries: Vec::new(),
            cursor_metronome_last_x: 0.0,
            cursor_metronome_last_y: 0.0,
            cursor_metronome_tick_start: None,
            cursor_ripple_ring_start: None,
            cursor_ripple_ring_last_x: 0.0,
            cursor_ripple_ring_last_y: 0.0,
            cursor_shockwave_start: None,
            cursor_shockwave_last_x: 0.0,
            cursor_shockwave_last_y: 0.0,
            cursor_bubble_spawn_time: None,
            cursor_bubble_last_x: 0.0,
            cursor_bubble_last_y: 0.0,
            cursor_firework_start: None,
            cursor_firework_last_x: 0.0,
            cursor_firework_last_y: 0.0,
            cursor_lightning_start: None,
            cursor_lightning_last_x: 0.0,
            cursor_lightning_last_y: 0.0,
            cursor_snowflake_start: None,
            cursor_snowflake_last_x: 0.0,
            cursor_snowflake_last_y: 0.0,
            edge_glow_entries: Vec::new(),
            rain_drops: Vec::new(),
            rain_last_spawn: std::time::Instant::now(),
            cursor_ripple_waves: Vec::new(),
            aurora_start: std::time::Instant::now(),
        }
    }

    async fn new_async(
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        // Create wgpu instance
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Request adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: crate::gpu_power_preference(),
                compatible_surface: surface.as_ref(),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "Failed to find a suitable GPU adapter".to_string())?;

        // Request device and queue
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Neomacs Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .map_err(|e| format!("Failed to create device: {}", e))?;

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
        Ok(Self::create_renderer_internal(device, queue, surface, surface_format, width, height, 1.0))
    }

    /// Resize the renderer's surface.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.width = width;
        self.height = height;

        // Update surface configuration
        if let (Some(surface), Some(config)) = (&self.surface, &mut self.surface_config) {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }

        // Update uniform buffer with logical size so vertex positions from Emacs map correctly
        let logical_w = width as f32 / self.scale_factor;
        let logical_h = height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Update the display scale factor (for multi-monitor DPI changes)
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
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
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost) => {
                self.resize(self.width, self.height);
                return;
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                log::error!("Out of GPU memory");
                return;
            }
            Err(e) => {
                log::warn!("Surface error: {:?}", e);
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.render_to_view(&view, scene);

        output.present();
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
            if let Some(cursor) = &window.cursor {
                if cursor.visible {
                    let cursor_x = window.bounds.x + cursor.x;
                    let cursor_y = window.bounds.y + cursor.y;

                    match cursor.style {
                        CursorStyle::Box => {
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
                        CursorStyle::Bar => {
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
                        CursorStyle::Underline => {
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
                        CursorStyle::Hollow => {
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
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.rect_pipeline);
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
        let bytes_per_row = ((pattern.width + 7) / 8) as usize;
        let w_pixels = width as u32;
        let h_pixels = height as u32;

        // Tile the pattern over the area, merging horizontal runs
        for py in 0..h_pixels {
            let pat_y = py % pattern.height;
            let mut px = 0u32;
            while px < w_pixels {
                let pat_x = px % pattern.width;
                let byte_idx = pat_y as usize * bytes_per_row + (pat_x / 8) as usize;
                let bit_idx = pat_x % 8;
                let bit_set = byte_idx < pattern.bits.len()
                    && (pattern.bits[byte_idx] >> bit_idx) & 1 != 0;
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
                    let set2 = bi2 < pattern.bits.len()
                        && (pattern.bits[bi2] >> bit2) & 1 != 0;
                    if !set2 { break; }
                    px += 1;
                }
                let run_len = px - run_start;
                self.add_rect(vertices, x + run_start as f32, y + py as f32,
                              run_len as f32, 1.0, fg);
            }
        }
    }

    /// Emit a single rounded-rectangle border as 6 vertices (one oversized quad).
    ///
    /// The quad is padded by 1px on each side so the SDF fragment shader has
    /// room for anti-aliased edges.  The shader carves out the interior, leaving
    /// only the border ring with rounded corners.
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
        let padding = 1.0; // extra pixels for SDF anti-aliasing fringe
        let x0 = x - padding;
        let y0 = y - padding;
        let x1 = x + width + padding;
        let y1 = y + height + padding;

        let rect_min = [x, y];
        let rect_max = [x + width, y + height];
        let params = [border_width, corner_radius];
        let color_arr = [color.r, color.g, color.b, color.a];

        let v = |px: f32, py: f32| RoundedRectVertex {
            position: [px, py],
            color: color_arr,
            rect_min,
            rect_max,
            params,
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
    fn add_quad(
        &self,
        vertices: &mut Vec<RectVertex>,
        corners: &[(f32, f32); 4],
        color: &Color,
    ) {
        let color_arr = [color.r, color.g, color.b, color.a];
        let [tl, tr, br, bl] = *corners;

        // Triangle 1: TL, TR, BL
        vertices.push(RectVertex { position: [tl.0, tl.1], color: color_arr });
        vertices.push(RectVertex { position: [tr.0, tr.1], color: color_arr });
        vertices.push(RectVertex { position: [bl.0, bl.1], color: color_arr });

        // Triangle 2: TR, BR, BL
        vertices.push(RectVertex { position: [tr.0, tr.1], color: color_arr });
        vertices.push(RectVertex { position: [br.0, br.1], color: color_arr });
        vertices.push(RectVertex { position: [bl.0, bl.1], color: color_arr });
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
        self.image_cache.bind_group_layout()
    }

    /// Get the image sampler (for creating bind groups for offscreen textures)
    pub fn image_sampler(&self) -> &wgpu::Sampler {
        self.image_cache.sampler()
    }

    /// Get the uniform bind group (needed for composite rendering)
    pub fn uniform_bind_group(&self) -> &wgpu::BindGroup {
        &self.uniform_bind_group
    }

    /// Get the image pipeline (needed for blit and scroll slide)
    pub fn image_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.image_pipeline
    }

    /// Create an offscreen texture suitable for rendering a full frame
    pub fn create_offscreen_texture(&self, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
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
            layout: self.image_cache.bind_group_layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(self.image_cache.sampler()),
                },
            ],
        })
    }

    /// Blit a texture to a target view (fullscreen quad)
    pub fn blit_texture_to_view(
        &self,
        src_bind_group: &wgpu::BindGroup,
        dst_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        // Use logical dimensions for vertex positions since screen_size uniform is logical
        let w = width as f32 / self.scale_factor;
        let h = height as f32 / self.scale_factor;

        let vertices = [
            GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [w, 0.0], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [0.0, h], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
        ];

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blit Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Blit Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_bind_group(1, src_bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    // ── Scroll Effect Implementations ─────────────────────────────────────

}
