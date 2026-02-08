//! wgpu GPU-accelerated scene renderer.

use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::core::face::{BoxType, Face, FaceAttributes};
use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use crate::core::scene::{CursorStyle, Scene};
use crate::core::types::{AnimatedCursor, Color, Rect};

use super::glyph_atlas::{GlyphKey, WgpuGlyphAtlas};
use super::image_cache::ImageCache;
#[cfg(feature = "video")]
use super::video_cache::VideoCache;
#[cfg(feature = "wpe-webkit")]
use super::webkit_cache::WgpuWebKitCache;
use super::vertex::{GlyphVertex, RectVertex, RoundedRectVertex, Uniforms};

/// GPU-accelerated renderer using wgpu.
pub struct WgpuRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    surface_format: wgpu::TextureFormat,
    rect_pipeline: wgpu::RenderPipeline,
    rounded_rect_pipeline: wgpu::RenderPipeline,
    corner_mask_pipeline: wgpu::RenderPipeline,
    glyph_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    opaque_image_pipeline: wgpu::RenderPipeline,
    glyph_bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    image_cache: ImageCache,
    #[cfg(feature = "video")]
    video_cache: VideoCache,
    #[cfg(feature = "wpe-webkit")]
    webkit_cache: WgpuWebKitCache,
    width: u32,
    height: u32,
    /// Display scale factor (physical pixels / logical pixels)
    scale_factor: f32,
    /// Scroll bar thumb corner radius ratio (0.0-1.0)
    scroll_bar_thumb_radius: f32,
    /// Scroll bar track opacity (0.0-1.0)
    scroll_bar_track_opacity: f32,
    /// Scroll bar hover brightness multiplier
    scroll_bar_hover_brightness: f32,
    /// Indent guide rendering enabled
    indent_guides_enabled: bool,
    /// Indent guide color (linear RGBA)
    indent_guide_color: (f32, f32, f32, f32),
    /// Rainbow indent guide enabled (overrides single color when active)
    indent_guide_rainbow_enabled: bool,
    /// Rainbow indent guide colors (linear RGBA, cycling by depth)
    indent_guide_rainbow_colors: Vec<(f32, f32, f32, f32)>,
    /// Current line highlight enabled
    line_highlight_enabled: bool,
    /// Current line highlight color (linear RGBA)
    line_highlight_color: (f32, f32, f32, f32),
    /// Visible whitespace enabled
    show_whitespace_enabled: bool,
    /// Visible whitespace color (linear RGBA)
    show_whitespace_color: (f32, f32, f32, f32),
    /// Inactive window dimming enabled
    inactive_dim_enabled: bool,
    /// Inactive window dimming opacity
    inactive_dim_opacity: f32,
    /// Per-window dim opacity for smooth fade transitions
    per_window_dim: std::collections::HashMap<i64, f32>,
    /// Last dim update time for smooth interpolation
    last_dim_tick: std::time::Instant,
    /// Flag: renderer needs continuous redraws (e.g. dim fade in progress)
    pub needs_continuous_redraw: bool,
    /// Mode-line separator style (0=none, 1=line, 2=shadow, 3=gradient)
    mode_line_separator_style: u32,
    /// Mode-line separator color (linear RGB)
    mode_line_separator_color: (f32, f32, f32),
    /// Mode-line separator height in pixels
    mode_line_separator_height: f32,
    /// Cursor glow enabled
    cursor_glow_enabled: bool,
    /// Cursor glow color (linear RGB)
    cursor_glow_color: (f32, f32, f32),
    /// Cursor glow radius
    cursor_glow_radius: f32,
    /// Cursor glow peak opacity
    cursor_glow_opacity: f32,
    /// Cursor pulse enabled (sinusoidal glow modulation)
    cursor_pulse_enabled: bool,
    /// Cursor pulse speed in Hz
    cursor_pulse_speed: f32,
    /// Cursor pulse minimum opacity multiplier (0.0-1.0)
    cursor_pulse_min_opacity: f32,
    /// Start time for pulse phase calculation
    cursor_pulse_start: std::time::Instant,
    /// Focus mode enabled (dim outside current paragraph)
    focus_mode_enabled: bool,
    /// Focus mode dimming opacity
    focus_mode_opacity: f32,
    /// Minimap enabled
    minimap_enabled: bool,
    /// Minimap column width in logical pixels
    minimap_width: f32,
    /// Typing ripple effect enabled
    typing_ripple_enabled: bool,
    /// Max ripple radius in pixels
    typing_ripple_max_radius: f32,
    /// Ripple duration in seconds
    typing_ripple_duration: f32,
    /// Active ripples: (center_x, center_y, spawn_instant)
    active_ripples: Vec<(f32, f32, std::time::Instant)>,
    /// Search highlight pulse enabled
    search_pulse_enabled: bool,
    /// Face ID of the isearch face
    search_pulse_face_id: u32,
    /// Pulse start time
    search_pulse_start: std::time::Instant,
    /// Vignette effect
    vignette_enabled: bool,
    vignette_intensity: f32,
    vignette_radius: f32,
    /// Zen mode (distraction-free centered content)
    zen_mode_enabled: bool,
    zen_mode_content_width_pct: f32,
    zen_mode_margin_opacity: f32,
    /// Background pattern style (0=none, 1=dots, 2=grid, 3=crosshatch)
    bg_pattern_style: u32,
    /// Pattern spacing in pixels
    bg_pattern_spacing: f32,
    /// Pattern color (sRGB)
    bg_pattern_color: (f32, f32, f32),
    /// Pattern opacity
    bg_pattern_opacity: f32,
    /// Line insertion/deletion animation
    line_animation_enabled: bool,
    line_animation_duration_ms: u32,
    active_line_anims: Vec<LineAnimEntry>,
    /// Header/mode-line shadow depth effect
    header_shadow_enabled: bool,
    header_shadow_intensity: f32,
    header_shadow_size: f32,
    /// Cursor color cycling (rainbow hue rotation)
    cursor_color_cycle_enabled: bool,
    cursor_color_cycle_speed: f32,
    cursor_color_cycle_saturation: f32,
    cursor_color_cycle_lightness: f32,
    cursor_color_cycle_start: std::time::Instant,
    /// Window switch highlight fade
    window_switch_fade_enabled: bool,
    window_switch_fade_duration_ms: u32,
    window_switch_fade_intensity: f32,
    /// Active window switch fades: (window_id, bounds, started, duration, intensity)
    active_window_fades: Vec<WindowFadeEntry>,
    /// Breadcrumb/path bar overlay
    breadcrumb_enabled: bool,
    breadcrumb_opacity: f32,
    /// Frosted glass effect on mode-lines
    /// Smooth border color transition
    border_transition_enabled: bool,
    border_transition_active_color: (f32, f32, f32),
    border_transition_duration: std::time::Duration,
    /// Per-window border transition state: (window_id, is_becoming_active, start_time)
    border_transitions: Vec<(i64, bool, std::time::Instant)>,
    /// Previous selected window for border transition detection
    prev_border_selected: i64,
    /// Buffer-local accent color strip
    accent_strip_enabled: bool,
    accent_strip_width: f32,
    /// Noise/film grain overlay
    noise_grain_enabled: bool,
    noise_grain_intensity: f32,
    noise_grain_size: f32,
    noise_grain_frame: u32,
    /// Window padding gradient (inner edge shading)
    padding_gradient_enabled: bool,
    padding_gradient_color: (f32, f32, f32),
    padding_gradient_opacity: f32,
    padding_gradient_width: f32,
    frosted_glass_enabled: bool,
    frosted_glass_opacity: f32,
    frosted_glass_blur: f32,
    /// Title fade animation
    title_fade_enabled: bool,
    title_fade_duration_ms: u32,
    /// Previous breadcrumb text per window (window_id -> file_name)
    prev_breadcrumb_text: std::collections::HashMap<i64, String>,
    /// Active title fades (window_id -> (old_text, new_text, start_time))
    active_title_fades: Vec<TitleFadeEntry>,
    /// Active window border glow
    window_glow_enabled: bool,
    window_glow_color: (f32, f32, f32),
    window_glow_radius: f32,
    window_glow_intensity: f32,
    /// Scroll progress indicator bar
    scroll_progress_enabled: bool,
    scroll_progress_height: f32,
    scroll_progress_color: (f32, f32, f32),
    scroll_progress_opacity: f32,
    /// Inactive window color tint
    inactive_tint_enabled: bool,
    inactive_tint_color: (f32, f32, f32),
    inactive_tint_opacity: f32,
}

/// Entry for an active window switch highlight fade
struct WindowFadeEntry {
    window_id: i64,
    bounds: Rect,
    started: std::time::Instant,
    duration: std::time::Duration,
    intensity: f32,
}

/// Entry for an active title/breadcrumb crossfade animation
struct TitleFadeEntry {
    window_id: i64,
    bounds: Rect,
    old_text: String,
    new_text: String,
    started: std::time::Instant,
    duration: std::time::Duration,
}

/// Entry for an active line insertion/deletion animation
struct LineAnimEntry {
    /// Window bounds where the animation is active
    window_bounds: Rect,
    /// Y position below which glyphs are displaced
    edit_y: f32,
    /// Initial Y offset (negative=insertion slide-down, positive=deletion slide-up)
    initial_offset: f32,
    /// When the animation started
    started: std::time::Instant,
    /// Duration of the animation
    duration: std::time::Duration,
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
        let rect_shader_source = include_str!("shaders/rect.wgsl");
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
        let rounded_rect_shader_source = include_str!("shaders/rounded_rect.wgsl");
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
        let glyph_shader_source = include_str!("shaders/glyph.wgsl");
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
        let image_shader_source = include_str!("shaders/image.wgsl");
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
            scroll_bar_thumb_radius: 0.4,
            scroll_bar_track_opacity: 0.6,
            scroll_bar_hover_brightness: 1.4,
            indent_guides_enabled: false,
            indent_guide_color: (0.3, 0.3, 0.3, 0.3),
            indent_guide_rainbow_enabled: false,
            indent_guide_rainbow_colors: Vec::new(),
            line_highlight_enabled: false,
            line_highlight_color: (0.2, 0.2, 0.3, 0.15),
            show_whitespace_enabled: false,
            show_whitespace_color: (0.4, 0.4, 0.4, 0.3),
            inactive_dim_enabled: false,
            inactive_dim_opacity: 0.15,
            per_window_dim: std::collections::HashMap::new(),
            last_dim_tick: std::time::Instant::now(),
            needs_continuous_redraw: false,
            mode_line_separator_style: 0,
            mode_line_separator_color: (0.0, 0.0, 0.0),
            mode_line_separator_height: 3.0,
            cursor_glow_enabled: false,
            cursor_glow_color: (0.4, 0.6, 1.0),
            cursor_glow_radius: 30.0,
            cursor_glow_opacity: 0.15,
            cursor_pulse_enabled: false,
            cursor_pulse_speed: 1.0,
            cursor_pulse_min_opacity: 0.3,
            cursor_pulse_start: std::time::Instant::now(),
            focus_mode_enabled: false,
            focus_mode_opacity: 0.4,
            minimap_enabled: false,
            minimap_width: 80.0,
            typing_ripple_enabled: false,
            typing_ripple_max_radius: 40.0,
            typing_ripple_duration: 0.3,
            active_ripples: Vec::new(),
            search_pulse_enabled: false,
            search_pulse_face_id: 0,
            search_pulse_start: std::time::Instant::now(),
            vignette_enabled: false,
            vignette_intensity: 0.3,
            vignette_radius: 50.0,
            zen_mode_enabled: false,
            zen_mode_content_width_pct: 60.0,
            zen_mode_margin_opacity: 0.3,
            bg_pattern_style: 0,
            bg_pattern_spacing: 20.0,
            bg_pattern_color: (0.5, 0.5, 0.5),
            bg_pattern_opacity: 0.05,
            line_animation_enabled: false,
            line_animation_duration_ms: 150,
            active_line_anims: Vec::new(),
            header_shadow_enabled: false,
            header_shadow_intensity: 0.3,
            header_shadow_size: 6.0,
            cursor_color_cycle_enabled: false,
            cursor_color_cycle_speed: 0.5,
            cursor_color_cycle_saturation: 0.8,
            cursor_color_cycle_lightness: 0.6,
            cursor_color_cycle_start: std::time::Instant::now(),
            window_switch_fade_enabled: false,
            window_switch_fade_duration_ms: 200,
            window_switch_fade_intensity: 0.15,
            active_window_fades: Vec::new(),
            breadcrumb_enabled: false,
            breadcrumb_opacity: 0.7,
            border_transition_enabled: false,
            border_transition_active_color: (0.4, 0.6, 1.0),
            border_transition_duration: std::time::Duration::from_millis(200),
            border_transitions: Vec::new(),
            prev_border_selected: 0,
            accent_strip_enabled: false,
            accent_strip_width: 3.0,
            noise_grain_enabled: false,
            noise_grain_intensity: 0.03,
            noise_grain_size: 2.0,
            noise_grain_frame: 0,
            padding_gradient_enabled: false,
            padding_gradient_color: (0.0, 0.0, 0.0),
            padding_gradient_opacity: 0.15,
            padding_gradient_width: 8.0,
            frosted_glass_enabled: false,
            frosted_glass_opacity: 0.3,
            frosted_glass_blur: 4.0,
            title_fade_enabled: false,
            title_fade_duration_ms: 300,
            prev_breadcrumb_text: std::collections::HashMap::new(),
            active_title_fades: Vec::new(),
            window_glow_enabled: false,
            window_glow_color: (0.4, 0.6, 1.0),
            window_glow_radius: 8.0,
            window_glow_intensity: 0.4,
            scroll_progress_enabled: false,
            scroll_progress_height: 2.0,
            scroll_progress_color: (0.4, 0.6, 1.0),
            scroll_progress_opacity: 0.8,
            inactive_tint_enabled: false,
            inactive_tint_color: (0.2, 0.1, 0.0),
            inactive_tint_opacity: 0.1,
        }
    }

    /// Update inactive window dim config
    pub fn set_inactive_dim_config(&mut self, enabled: bool, opacity: f32) {
        self.inactive_dim_enabled = enabled;
        self.inactive_dim_opacity = opacity;
    }

    /// Update mode-line separator config
    pub fn set_mode_line_separator(&mut self, style: u32, color: (f32, f32, f32), height: f32) {
        self.mode_line_separator_style = style;
        self.mode_line_separator_color = color;
        self.mode_line_separator_height = height;
    }

    /// Update cursor glow config
    pub fn set_cursor_glow(&mut self, enabled: bool, color: (f32, f32, f32), radius: f32, opacity: f32) {
        self.cursor_glow_enabled = enabled;
        self.cursor_glow_color = color;
        self.cursor_glow_radius = radius;
        self.cursor_glow_opacity = opacity;
    }

    /// Update cursor pulse config
    pub fn set_cursor_pulse(&mut self, enabled: bool, speed: f32, min_opacity: f32) {
        self.cursor_pulse_enabled = enabled;
        self.cursor_pulse_speed = speed;
        self.cursor_pulse_min_opacity = min_opacity;
    }

    /// Update focus mode config
    pub fn set_focus_mode(&mut self, enabled: bool, opacity: f32) {
        self.focus_mode_enabled = enabled;
        self.focus_mode_opacity = opacity;
    }

    /// Update minimap config
    pub fn set_minimap(&mut self, enabled: bool, width: f32) {
        self.minimap_enabled = enabled;
        self.minimap_width = width;
    }

    /// Update vignette config
    pub fn set_vignette(&mut self, enabled: bool, intensity: f32, radius: f32) {
        self.vignette_enabled = enabled;
        self.vignette_intensity = intensity;
        self.vignette_radius = radius;
    }

    /// Update zen mode config
    pub fn set_zen_mode(&mut self, enabled: bool, content_width_pct: f32, margin_opacity: f32) {
        self.zen_mode_enabled = enabled;
        self.zen_mode_content_width_pct = content_width_pct;
        self.zen_mode_margin_opacity = margin_opacity;
    }

    /// Update background pattern config
    pub fn set_background_pattern(&mut self, style: u32, spacing: f32, r: f32, g: f32, b: f32, opacity: f32) {
        self.bg_pattern_style = style;
        self.bg_pattern_spacing = spacing;
        self.bg_pattern_color = (r, g, b);
        self.bg_pattern_opacity = opacity;
    }

    /// Update line animation config
    pub fn set_line_animation(&mut self, enabled: bool, duration_ms: u32) {
        self.line_animation_enabled = enabled;
        self.line_animation_duration_ms = duration_ms;
        if !enabled {
            self.active_line_anims.clear();
        }
    }

    /// Start a line animation for a window
    pub fn start_line_animation(&mut self, window_bounds: Rect, edit_y: f32, offset: f32, duration_ms: u32) {
        // Remove any existing animation for this window region
        self.active_line_anims.retain(|a| {
            (a.window_bounds.x - window_bounds.x).abs() > 1.0
            || (a.window_bounds.y - window_bounds.y).abs() > 1.0
        });
        self.active_line_anims.push(LineAnimEntry {
            window_bounds,
            edit_y,
            initial_offset: offset,
            started: std::time::Instant::now(),
            duration: std::time::Duration::from_millis(duration_ms as u64),
        });
        self.needs_continuous_redraw = true;
    }

    /// Compute Y offset for a glyph due to active line animations
    fn line_y_offset(&self, gx: f32, gy: f32) -> f32 {
        for anim in &self.active_line_anims {
            let b = &anim.window_bounds;
            // Check if glyph is in this window and below the edit point
            if gx >= b.x && gx < b.x + b.width
                && gy >= b.y && gy < b.y + b.height
                && gy >= anim.edit_y
            {
                let elapsed = anim.started.elapsed();
                let t = (elapsed.as_secs_f32() / anim.duration.as_secs_f32()).min(1.0);
                // Ease-out quadratic: t * (2 - t)
                let eased = t * (2.0 - t);
                return anim.initial_offset * (1.0 - eased);
            }
        }
        0.0
    }

    /// Update cursor color cycling config
    pub fn set_cursor_color_cycle(&mut self, enabled: bool, speed: f32, saturation: f32, lightness: f32) {
        self.cursor_color_cycle_enabled = enabled;
        self.cursor_color_cycle_speed = speed;
        self.cursor_color_cycle_saturation = saturation;
        self.cursor_color_cycle_lightness = lightness;
        if enabled {
            self.cursor_color_cycle_start = std::time::Instant::now();
        }
    }

    /// Update window switch fade config
    pub fn set_window_switch_fade(&mut self, enabled: bool, duration_ms: u32, intensity: f32) {
        self.window_switch_fade_enabled = enabled;
        self.window_switch_fade_duration_ms = duration_ms;
        self.window_switch_fade_intensity = intensity;
    }

    /// Update inactive window tint config
    pub fn set_inactive_tint(&mut self, enabled: bool, color: (f32, f32, f32), opacity: f32) {
        self.inactive_tint_enabled = enabled;
        self.inactive_tint_color = color;
        self.inactive_tint_opacity = opacity;
    }

    /// Update scroll progress indicator config
    pub fn set_scroll_progress(&mut self, enabled: bool, height: f32, color: (f32, f32, f32), opacity: f32) {
        self.scroll_progress_enabled = enabled;
        self.scroll_progress_height = height;
        self.scroll_progress_color = color;
        self.scroll_progress_opacity = opacity;
    }

    /// Update active window border glow config
    pub fn set_window_glow(&mut self, enabled: bool, color: (f32, f32, f32), radius: f32, intensity: f32) {
        self.window_glow_enabled = enabled;
        self.window_glow_color = color;
        self.window_glow_radius = radius;
        self.window_glow_intensity = intensity;
    }

    /// Update breadcrumb/path bar config
    pub fn set_breadcrumb(&mut self, enabled: bool, opacity: f32) {
        self.breadcrumb_enabled = enabled;
        self.breadcrumb_opacity = opacity;
    }

    /// Update border transition config
    pub fn set_border_transition(&mut self, enabled: bool, active_color: (f32, f32, f32), duration_ms: u32) {
        self.border_transition_enabled = enabled;
        self.border_transition_active_color = active_color;
        self.border_transition_duration = std::time::Duration::from_millis(duration_ms as u64);
        if !enabled {
            self.border_transitions.clear();
        }
    }

    /// Update accent strip config
    pub fn set_accent_strip(&mut self, enabled: bool, width: f32) {
        self.accent_strip_enabled = enabled;
        self.accent_strip_width = width;
    }

    /// Update noise grain config
    pub fn set_noise_grain(&mut self, enabled: bool, intensity: f32, size: f32) {
        self.noise_grain_enabled = enabled;
        self.noise_grain_intensity = intensity;
        self.noise_grain_size = size;
    }

    /// Update padding gradient config
    pub fn set_padding_gradient(&mut self, enabled: bool, color: (f32, f32, f32), opacity: f32, width: f32) {
        self.padding_gradient_enabled = enabled;
        self.padding_gradient_color = color;
        self.padding_gradient_opacity = opacity;
        self.padding_gradient_width = width;
    }

    /// Update frosted glass config
    pub fn set_frosted_glass(&mut self, enabled: bool, opacity: f32, blur: f32) {
        self.frosted_glass_enabled = enabled;
        self.frosted_glass_opacity = opacity;
        self.frosted_glass_blur = blur;
    }

    /// Update title fade config
    pub fn set_title_fade(&mut self, enabled: bool, duration_ms: u32) {
        self.title_fade_enabled = enabled;
        self.title_fade_duration_ms = duration_ms;
        if !enabled {
            self.active_title_fades.clear();
        }
    }

    /// Start a window switch fade for a specific window
    pub fn start_window_fade(&mut self, window_id: i64, bounds: Rect) {
        // Remove any existing fade for this window
        self.active_window_fades.retain(|f| f.window_id != window_id);
        self.active_window_fades.push(WindowFadeEntry {
            window_id,
            bounds,
            started: std::time::Instant::now(),
            duration: std::time::Duration::from_millis(self.window_switch_fade_duration_ms as u64),
            intensity: self.window_switch_fade_intensity,
        });
    }

    /// Convert HSL to sRGB Color
    fn hsl_to_color(h: f32, s: f32, l: f32) -> Color {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let (r, g, b) = match (h * 6.0) as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        Color { r: r + m, g: g + m, b: b + m, a: 1.0 }
    }

    /// Update header/mode-line shadow config
    pub fn set_header_shadow(&mut self, enabled: bool, intensity: f32, size: f32) {
        self.header_shadow_enabled = enabled;
        self.header_shadow_intensity = intensity;
        self.header_shadow_size = size;
    }

    /// Update search pulse config
    pub fn set_search_pulse(&mut self, enabled: bool, face_id: u32) {
        self.search_pulse_enabled = enabled;
        self.search_pulse_face_id = face_id;
        if enabled {
            self.search_pulse_start = std::time::Instant::now();
        }
    }

    /// Update typing ripple config
    pub fn set_typing_ripple(&mut self, enabled: bool, max_radius: f32, duration_ms: u32) {
        self.typing_ripple_enabled = enabled;
        self.typing_ripple_max_radius = max_radius;
        self.typing_ripple_duration = duration_ms as f32 / 1000.0;
        if !enabled {
            self.active_ripples.clear();
        }
    }

    /// Spawn a new ripple at the given position
    pub fn spawn_ripple(&mut self, cx: f32, cy: f32) {
        if self.typing_ripple_enabled {
            self.active_ripples.push((cx, cy, std::time::Instant::now()));
        }
    }

    /// Update visible whitespace config
    pub fn set_show_whitespace_config(&mut self, enabled: bool, color: (f32, f32, f32, f32)) {
        self.show_whitespace_enabled = enabled;
        self.show_whitespace_color = color;
    }

    /// Update line highlight config
    pub fn set_line_highlight_config(&mut self, enabled: bool, color: (f32, f32, f32, f32)) {
        self.line_highlight_enabled = enabled;
        self.line_highlight_color = color;
    }

    /// Update indent guide config
    pub fn set_indent_guide_config(&mut self, enabled: bool, color: (f32, f32, f32, f32)) {
        self.indent_guides_enabled = enabled;
        self.indent_guide_color = color;
    }

    /// Update rainbow indent guide config
    pub fn set_indent_guide_rainbow(&mut self, enabled: bool, colors: Vec<(f32, f32, f32, f32)>) {
        self.indent_guide_rainbow_enabled = enabled;
        self.indent_guide_rainbow_colors = colors;
    }

    /// Update scroll bar rendering config
    pub fn set_scroll_bar_config(&mut self, thumb_radius: f32, track_opacity: f32,
                                  hover_brightness: f32) {
        self.scroll_bar_thumb_radius = thumb_radius;
        self.scroll_bar_track_opacity = track_opacity;
        self.scroll_bar_hover_brightness = hover_brightness;
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

    /// Load image from file path (async - returns immediately)
    /// Returns image ID, actual texture loads in background
    pub fn load_image_file(&mut self, path: &str, max_width: u32, max_height: u32) -> u32 {
        self.image_cache.load_file(path, max_width, max_height)
    }

    /// Load image from file path with a pre-allocated ID (for threaded mode)
    pub fn load_image_file_with_id(&mut self, id: u32, path: &str, max_width: u32, max_height: u32) {
        self.image_cache.load_file_with_id(id, path, max_width, max_height)
    }

    /// Load image from data (async - returns immediately)
    pub fn load_image_data(&mut self, data: &[u8], max_width: u32, max_height: u32) -> u32 {
        self.image_cache.load_data(data, max_width, max_height)
    }

    /// Load image from raw ARGB32 pixel data
    pub fn load_image_argb32(&mut self, data: &[u8], width: u32, height: u32, stride: u32) -> u32 {
        self.image_cache.load_raw_argb32(data, width, height, stride, 0, 0)
    }

    /// Load image from raw RGB24 pixel data
    pub fn load_image_rgb24(&mut self, data: &[u8], width: u32, height: u32, stride: u32) -> u32 {
        self.image_cache.load_raw_rgb24(data, width, height, stride, 0, 0)
    }

    /// Query image file dimensions (fast - reads header only, does not block)
    pub fn query_image_file_size(path: &str) -> Option<(u32, u32)> {
        ImageCache::query_file_dimensions(path).map(|d| (d.width, d.height))
    }

    /// Query image data dimensions (fast - reads header only)
    pub fn query_image_data_size(data: &[u8]) -> Option<(u32, u32)> {
        ImageCache::query_data_dimensions(data).map(|d| (d.width, d.height))
    }

    /// Get image dimensions (works for pending and loaded images)
    pub fn get_image_size(&self, id: u32) -> Option<(u32, u32)> {
        self.image_cache.get_dimensions(id).map(|d| (d.width, d.height))
    }

    /// Check if image is ready for rendering
    pub fn is_image_ready(&self, id: u32) -> bool {
        self.image_cache.is_ready(id)
    }

    /// Free an image from cache
    pub fn free_image(&mut self, id: u32) {
        self.image_cache.free(id)
    }

    /// Process pending decoded images (call each frame before rendering)
    pub fn process_pending_images(&mut self) {
        self.image_cache.process_pending(&self.device, &self.queue);
    }

    // =========== Video Loading Methods ===========

    /// Load video from file path (async - returns immediately)
    /// Returns video ID, frames decode in background
    #[cfg(feature = "video")]
    pub fn load_video_file(&mut self, path: &str) -> u32 {
        self.video_cache.load_file(path)
    }

    /// Get video dimensions
    #[cfg(feature = "video")]
    pub fn get_video_size(&self, id: u32) -> Option<(u32, u32)> {
        self.video_cache.get_dimensions(id)
    }

    /// Get video state
    #[cfg(feature = "video")]
    pub fn get_video_state(&self, id: u32) -> Option<super::video_cache::VideoState> {
        self.video_cache.get_state(id)
    }

    /// Play video
    #[cfg(feature = "video")]
    pub fn video_play(&mut self, id: u32) {
        self.video_cache.play(id)
    }

    /// Pause video
    #[cfg(feature = "video")]
    pub fn video_pause(&mut self, id: u32) {
        self.video_cache.pause(id)
    }

    /// Stop video
    #[cfg(feature = "video")]
    pub fn video_stop(&mut self, id: u32) {
        self.video_cache.stop(id)
    }

    /// Set video loop count (-1 for infinite)
    #[cfg(feature = "video")]
    pub fn video_set_loop(&mut self, id: u32, count: i32) {
        self.video_cache.set_loop(id, count)
    }

    /// Free a video from cache
    #[cfg(feature = "video")]
    pub fn free_video(&mut self, id: u32) {
        self.video_cache.remove(id)
    }

    /// Process pending decoded video frames (call each frame before rendering)
    #[cfg(feature = "video")]
    pub fn process_pending_videos(&mut self) {
        log::debug!("process_pending_videos called");
        // Use image_cache's bind_group_layout and sampler to ensure video bind groups
        // are compatible with the shared image/video rendering pipeline
        let layout = self.image_cache.bind_group_layout();
        let sampler = self.image_cache.sampler();
        self.video_cache.process_pending(&self.device, &self.queue, layout, sampler);
    }

    /// Check if any video is currently playing
    #[cfg(feature = "video")]
    pub fn has_playing_videos(&self) -> bool {
        self.video_cache.has_playing_videos()
    }

    /// Get cached video for rendering
    #[cfg(feature = "video")]
    pub fn get_video(&self, id: u32) -> Option<&super::video_cache::CachedVideo> {
        self.video_cache.get(id)
    }

    /// Update a webkit view in the cache from a DMA-BUF buffer.
    /// Returns true if successful.
    #[cfg(feature = "wpe-webkit")]
    pub fn update_webkit_view_dmabuf(
        &mut self,
        view_id: u32,
        buffer: super::external_buffer::DmaBufBuffer,
    ) -> bool {
        self.webkit_cache.update_view(view_id, buffer, &self.device, &self.queue)
    }

    /// Update a webkit view in the cache from pixel data.
    /// Returns true if successful.
    #[cfg(feature = "wpe-webkit")]
    pub fn update_webkit_view_pixels(
        &mut self,
        view_id: u32,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> bool {
        self.webkit_cache.update_view_from_pixels(view_id, width, height, pixels, &self.device, &self.queue)
    }

    /// Remove a webkit view from the cache.
    #[cfg(feature = "wpe-webkit")]
    pub fn remove_webkit_view(&mut self, view_id: u32) {
        self.webkit_cache.remove(view_id);
    }

    /// Process pending webkit frames from WPE views.
    /// NOTE: In threaded mode, frame processing is done in render_thread.rs
    /// which calls update_webkit_view_dmabuf/update_webkit_view_pixels directly.
    /// This method is kept for API compatibility but is a no-op.
    #[cfg(feature = "wpe-webkit")]
    pub fn process_webkit_frames(&mut self) {
        // In threaded mode, frame processing happens in render_thread.rs
        // The render thread calls update_webkit_view_dmabuf/update_webkit_view_pixels directly
    }

    /// Render floating webkit views to the screen.
    /// This draws the cached webkit textures at their specified positions.
    #[cfg(feature = "wpe-webkit")]
    pub fn render_floating_webkits(
        &self,
        view: &wgpu::TextureView,
        floating_webkits: &[crate::core::scene::FloatingWebKit],
    ) {
        use wgpu::util::DeviceExt;

        if floating_webkits.is_empty() {
            return;
        }

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Floating WebKit Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Floating WebKit Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Preserve existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.opaque_image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            for fw in floating_webkits {
                log::debug!("Rendering floating webkit {} at ({}, {}) size {}x{}",
                           fw.webkit_id, fw.x, fw.y, fw.width, fw.height);

                if let Some(cached) = self.webkit_cache.get(fw.webkit_id) {
                    let vertices = [
                        GlyphVertex { position: [fw.x, fw.y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                        GlyphVertex { position: [fw.x + fw.width, fw.y], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                        GlyphVertex { position: [fw.x + fw.width, fw.y + fw.height], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                        GlyphVertex { position: [fw.x, fw.y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                        GlyphVertex { position: [fw.x + fw.width, fw.y + fw.height], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                        GlyphVertex { position: [fw.x, fw.y + fw.height], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                    ];

                    let webkit_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Floating WebKit Vertex Buffer"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

                    render_pass.set_bind_group(1, &cached.bind_group, &[]);
                    render_pass.set_vertex_buffer(0, webkit_buffer.slice(..));
                    render_pass.draw(0..6, 0..1);
                } else {
                    log::debug!("WebKit {} not found in cache", fw.webkit_id);
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));
    }

    /// Render frame glyphs to a texture view
    ///
    /// `surface_width` and `surface_height` should be the actual surface dimensions
    /// for correct coordinate transformation.
    pub fn render_frame_glyphs(
        &mut self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        faces: &HashMap<u32, Face>,
        surface_width: u32,
        surface_height: u32,
        cursor_visible: bool,
        animated_cursor: Option<AnimatedCursor>,
        mouse_pos: (f32, f32),
        background_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
    ) {
        log::debug!(
            "render_frame_glyphs: frame={}x{} surface={}x{}, {} glyphs, {} faces",
            frame_glyphs.width,
            frame_glyphs.height,
            surface_width,
            surface_height,
            frame_glyphs.glyphs.len(),
            faces.len(),
        );

        // Reset continuous redraw flag (will be set by dim fade or other animations)
        self.needs_continuous_redraw = false;

        // Clean up expired line animations
        self.active_line_anims.retain(|a| a.started.elapsed() < a.duration);
        if !self.active_line_anims.is_empty() {
            self.needs_continuous_redraw = true;
        }

        // Advance glyph atlas generation for LRU tracking
        glyph_atlas.advance_generation();

        // Update uniforms with logical size for correct coordinate transformation
        // (Emacs sends positions in logical pixels; surface is in physical pixels)
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Rendering order for correct z-layering (inverse video cursor):
        //   1. Non-overlay backgrounds (window bg, stretches, char bg)
        //   2. Cursor bg rect (inverse video background for filled box cursor)
        //   3. Animated cursor trail (behind text, for filled box cursor motion)
        //   4. Non-overlay text (with cursor_fg swap for char at cursor position)
        //   5. Overlay backgrounds (mode-line/echo bg)
        //   6. Overlay text (mode-line/echo text)
        //   7. Inline media (images, videos, webkits)
        //   8. Front cursors (bar, hbar, hollow) and borders
        //
        // Filled box cursor (style 0) is split across steps 2-4 for inverse video.
        // Bar/hbar/hollow cursors are drawn on top of text in step 8.

        // Find minimum Y of overlay chars (mode-line/echo-area) for clipping inline media
        let overlay_y: Option<f32> = frame_glyphs.glyphs.iter()
            .filter_map(|g| {
                if let FrameGlyph::Char { y, is_overlay: true, .. } = g {
                    if *y < frame_glyphs.height {
                        Some(*y)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .reduce(f32::min);
        log::trace!("Frame {}x{}, overlay_y={:?}", frame_glyphs.width, frame_glyphs.height, overlay_y);

        // --- Merge adjacent boxed glyphs into spans ---
        // All box faces get span-merged for proper border rendering.
        // Only faces with corner_radius > 0 get the SDF rounded rect treatment
        // (background suppression + SDF fill + SDF border).
        // Standard boxes (corner_radius=0) get merged rect borders drawn after text.
        struct BoxSpan {
            x: f32,
            y: f32,
            width: f32,
            height: f32,
            face_id: u32,
            is_overlay: bool,
            bg: Option<Color>,
        }
        let mut box_spans: Vec<BoxSpan> = Vec::new();

        for glyph in &frame_glyphs.glyphs {
            // Extract position info from both Char and Stretch glyphs with box faces
            let (gx, gy, gw, gh, gface_id, g_overlay, g_bg) = match glyph {
                FrameGlyph::Char { x, y, width, height, face_id, is_overlay, bg, .. } => {
                    (*x, *y, *width, *height, *face_id, *is_overlay, *bg)
                }
                FrameGlyph::Stretch { x, y, width, height, face_id, is_overlay, bg } => {
                    (*x, *y, *width, *height, *face_id, *is_overlay, Some(*bg))
                }
                _ => continue,
            };

            // Only include glyphs whose face has BOX attribute
            match faces.get(&gface_id) {
                Some(f) if f.attributes.contains(FaceAttributes::BOX) && f.box_line_width > 0 => {}
                _ => continue,
            };

            // Check if this glyph's face has rounded corners
            let is_rounded = faces.get(&gface_id)
                .map(|f| f.box_corner_radius > 0)
                .unwrap_or(false);

            let merged = if let Some(last) = box_spans.last_mut() {
                let same_row = (last.y - gy).abs() < 0.5
                    && (last.height - gh).abs() < 0.5;
                let same_overlay = last.is_overlay == g_overlay;
                let adjacent = (gx - (last.x + last.width)).abs() < 1.0;
                let same_face = last.face_id == gface_id;

                // Merge rules:
                // - Rounded boxes: only merge same face_id (keep separate boxes)
                // - Sharp overlay boxes (mode-line): merge across face_ids (continuity)
                // - Sharp non-overlay boxes (content): only merge same face_id
                let last_is_rounded = faces.get(&last.face_id)
                    .map(|f| f.box_corner_radius > 0)
                    .unwrap_or(false);
                let face_ok = if is_rounded || last_is_rounded {
                    same_face  // rounded: strict same-face merge
                } else if g_overlay {
                    true  // sharp overlay: merge across faces (mode-line)
                } else {
                    same_face  // sharp non-overlay: strict same-face merge
                };

                if same_row && same_overlay && adjacent && face_ok {
                    last.width = gx + gw - last.x;
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if !merged {
                box_spans.push(BoxSpan {
                    x: gx, y: gy, width: gw, height: gh,
                    face_id: gface_id, is_overlay: g_overlay,
                    bg: g_bg,
                });
            }
        }

        // Helper: test whether a glyph position overlaps any ROUNDED box span.
        // Only suppresses backgrounds for rounded boxes (corner_radius > 0).
        // Standard boxes (corner_radius=0) keep normal rect backgrounds.
        let box_margin: f32 = box_spans.iter()
            .filter_map(|s| faces.get(&s.face_id)
                .filter(|f| f.box_corner_radius > 0)
                .map(|f| f.box_line_width as f32))
            .fold(0.0_f32, f32::max);
        let overlaps_rounded_box_span = |gx: f32, gy: f32, g_overlay: bool, spans: &[BoxSpan]| -> bool {
            if box_margin <= 0.0 { return false; }
            spans.iter().any(|s| {
                // Only check rounded box spans with the same overlay status
                if s.is_overlay != g_overlay { return false; }
                let is_rounded = faces.get(&s.face_id)
                    .map(|f| f.box_corner_radius > 0)
                    .unwrap_or(false);
                if !is_rounded { return false; }
                gx >= s.x - box_margin - 0.5 && gx < s.x + s.width + box_margin + 0.5
                && gy >= s.y - box_margin - 0.5 && gy < s.y + s.height + box_margin + 0.5
            })
        };

        // --- Collect non-overlay backgrounds ---
        let mut non_overlay_rect_vertices: Vec<RectVertex> = Vec::new();

        // Background gradient (rendered behind everything)
        if let Some((top, bottom)) = background_gradient {
            let top_color = Color::new(top.0, top.1, top.2, 1.0).srgb_to_linear();
            let bot_color = Color::new(bottom.0, bottom.1, bottom.2, 1.0).srgb_to_linear();
            let tc = [top_color.r, top_color.g, top_color.b, top_color.a];
            let bc = [bot_color.r, bot_color.g, bot_color.b, bot_color.a];
            // Two triangles forming a fullscreen quad with gradient
            // Top-left, top-right, bottom-left (triangle 1)
            non_overlay_rect_vertices.push(RectVertex { position: [0.0, 0.0], color: tc });
            non_overlay_rect_vertices.push(RectVertex { position: [logical_w, 0.0], color: tc });
            non_overlay_rect_vertices.push(RectVertex { position: [0.0, logical_h], color: bc });
            // Top-right, bottom-right, bottom-left (triangle 2)
            non_overlay_rect_vertices.push(RectVertex { position: [logical_w, 0.0], color: tc });
            non_overlay_rect_vertices.push(RectVertex { position: [logical_w, logical_h], color: bc });
            non_overlay_rect_vertices.push(RectVertex { position: [0.0, logical_h], color: bc });
        }

        // Window backgrounds
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Background { bounds, color } = glyph {
                self.add_rect(
                    &mut non_overlay_rect_vertices,
                    bounds.x, bounds.y, bounds.width, bounds.height, color,
                );
            }
        }
        // Non-overlay stretches (skip those inside a box span)
        let has_line_anims = !self.active_line_anims.is_empty();
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Stretch { x, y, width, height, bg, is_overlay, .. } = glyph {
                if !*is_overlay && !overlaps_rounded_box_span(*x, *y, false, &box_spans) {
                    let ya = if has_line_anims { *y + self.line_y_offset(*x, *y) } else { *y };
                    self.add_rect(&mut non_overlay_rect_vertices, *x, ya, *width, *height, bg);
                }
            }
        }
        // Non-overlay char backgrounds (skip boxed chars — they get rounded bg instead)
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Char { x, y, width, height, bg, is_overlay, .. } = glyph {
                if !*is_overlay {
                    if let Some(bg_color) = bg {
                        if !overlaps_rounded_box_span(*x, *y, false, &box_spans) {
                            let ya = if has_line_anims { *y + self.line_y_offset(*x, *y) } else { *y };
                            self.add_rect(&mut non_overlay_rect_vertices, *x, ya, *width, *height, bg_color);
                        }
                    }
                }
            }
        }

        // --- Current line highlight ---
        if self.line_highlight_enabled {
            let (lr, lg, lb, la) = self.line_highlight_color;
            let hl_color = Color::new(lr, lg, lb, la);

            // Find the active cursor (style != 3, which is hollow/inactive)
            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Cursor { y, height, style, .. } = glyph {
                    if *style != 3 {
                        // Find the window this cursor belongs to
                        for info in &frame_glyphs.window_infos {
                            if info.selected {
                                // Draw highlight across the window width (excluding mode-line)
                                let hl_y = *y;
                                let hl_h = *height;
                                self.add_rect(
                                    &mut non_overlay_rect_vertices,
                                    info.bounds.x, hl_y,
                                    info.bounds.width, hl_h,
                                    &hl_color,
                                );
                                break;
                            }
                        }
                        break;
                    }
                }
            }
        }

        // --- Indent guides ---
        if self.indent_guides_enabled {
            let (ig_r, ig_g, ig_b, ig_a) = self.indent_guide_color;
            let guide_color = Color::new(ig_r, ig_g, ig_b, ig_a);
            let guide_width = 1.0_f32;

            // Detect char_width from frame
            let char_w = frame_glyphs.char_width.max(1.0);
            let tab_w = 4; // default tab width; we infer from the glyph spacing

            // Collect row info: group chars by Y coordinate to find rows,
            // then detect indent (leading space/tab) per row.
            struct RowInfo {
                y: f32,
                height: f32,
                first_non_space_x: f32,
                text_start_x: f32, // leftmost char X in the row
            }
            let mut rows: Vec<RowInfo> = Vec::new();
            let mut current_row_y: f32 = -1.0;
            let mut current_row_h: f32 = 0.0;
            let mut first_non_space_x: f32 = f32::MAX;
            let mut text_start_x: f32 = f32::MAX;
            let mut has_chars = false;

            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Char { x, y, width, height, char: ch, is_overlay, .. } = glyph {
                    if *is_overlay { continue; }
                    let gy = *y;
                    if (gy - current_row_y).abs() > 0.5 {
                        // New row — save previous
                        if has_chars && first_non_space_x > text_start_x + char_w {
                            rows.push(RowInfo {
                                y: current_row_y,
                                height: current_row_h,
                                first_non_space_x,
                                text_start_x,
                            });
                        }
                        current_row_y = gy;
                        current_row_h = *height;
                        first_non_space_x = f32::MAX;
                        text_start_x = f32::MAX;
                        has_chars = false;
                    }
                    has_chars = true;
                    if *x < text_start_x { text_start_x = *x; }
                    if *ch != ' ' && *ch != '\t' && *x < first_non_space_x {
                        first_non_space_x = *x;
                    }
                }
            }
            // Save last row
            if has_chars && first_non_space_x > text_start_x + char_w {
                rows.push(RowInfo {
                    y: current_row_y,
                    height: current_row_h,
                    first_non_space_x,
                    text_start_x,
                });
            }

            // Draw guides at each tab-stop column within the indent region
            let tab_px = char_w * tab_w as f32;
            let use_rainbow = self.indent_guide_rainbow_enabled
                && !self.indent_guide_rainbow_colors.is_empty();
            for row in &rows {
                let mut col_x = row.text_start_x + tab_px;
                let mut depth: usize = 0;
                while col_x < row.first_non_space_x - 1.0 {
                    let color = if use_rainbow {
                        let (r, g, b, a) = self.indent_guide_rainbow_colors
                            [depth % self.indent_guide_rainbow_colors.len()];
                        Color::new(r, g, b, a)
                    } else {
                        guide_color
                    };
                    self.add_rect(
                        &mut non_overlay_rect_vertices,
                        col_x, row.y, guide_width, row.height,
                        &color,
                    );
                    col_x += tab_px;
                    depth += 1;
                }
            }
        }

        // --- Visible whitespace dots ---
        if self.show_whitespace_enabled {
            let (wr, wg, wb, wa) = self.show_whitespace_color;
            let ws_color = Color::new(wr, wg, wb, wa);
            let dot_size = 1.5_f32;

            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Char { char: ch, x, y, width, height, ascent, is_overlay, .. } = glyph {
                    if *is_overlay { continue; }
                    if *ch == ' ' {
                        // Centered dot for space
                        let dot_x = *x + (*width - dot_size) / 2.0;
                        let dot_y = *y + (*ascent - dot_size / 2.0);
                        self.add_rect(
                            &mut non_overlay_rect_vertices,
                            dot_x, dot_y, dot_size, dot_size,
                            &ws_color,
                        );
                    } else if *ch == '\t' {
                        // Small horizontal arrow for tab
                        let arrow_h = 1.5_f32;
                        let arrow_y = *y + (*ascent - arrow_h / 2.0);
                        let arrow_w = (*width - 4.0).max(4.0);
                        let arrow_x = *x + 2.0;
                        // Shaft
                        self.add_rect(
                            &mut non_overlay_rect_vertices,
                            arrow_x, arrow_y, arrow_w, arrow_h,
                            &ws_color,
                        );
                        // Arrowhead (small triangle approximated as 2 rects)
                        let tip_x = arrow_x + arrow_w;
                        self.add_rect(
                            &mut non_overlay_rect_vertices,
                            tip_x - 3.0, arrow_y - 1.5, 3.0, arrow_h + 3.0,
                            &ws_color,
                        );
                    }
                }
            }
        }

        // --- Collect overlay backgrounds ---
        let mut overlay_rect_vertices: Vec<RectVertex> = Vec::new();

        // Overlay stretches (skip those inside a box span)
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Stretch { x, y, width, height, bg, is_overlay, .. } = glyph {
                if *is_overlay && !overlaps_rounded_box_span(*x, *y, true, &box_spans) {
                    self.add_rect(&mut overlay_rect_vertices, *x, *y, *width, *height, bg);
                }
            }
        }
        // Overlay char backgrounds (skip those inside a box span)
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Char { x, y, width, height, bg, is_overlay, .. } = glyph {
                if *is_overlay {
                    if let Some(bg_color) = bg {
                        if !overlaps_rounded_box_span(*x, *y, true, &box_spans) {
                            self.add_rect(&mut overlay_rect_vertices, *x, *y, *width, *height, bg_color);
                        }
                    }
                }
            }
        }

        // === Collect cursor bg rect for inverse video (drawn before text) ===
        // For filled box cursor (style 0), we draw the cursor background BEFORE text
        // so the character under the cursor can be re-drawn with inverse colors on top.
        let mut cursor_bg_vertices: Vec<RectVertex> = Vec::new();

        // === Collect behind-text cursor shapes (animated trail for filled box) ===
        let mut behind_text_cursor_vertices: Vec<RectVertex> = Vec::new();

        // === Collect front cursors and borders (drawn after text) ===
        // Bar (1), hbar (2), hollow (3), borders — all drawn on top of text.
        // Filled box (0) is EXCLUDED here — handled by bg rect + trail + fg swap.
        let mut cursor_vertices: Vec<RectVertex> = Vec::new();

        // === Collect scroll bar thumbs (drawn as rounded rects) ===
        let mut scroll_bar_thumb_vertices: Vec<(f32, f32, f32, f32, f32, Color)> = Vec::new();

        for glyph in &frame_glyphs.glyphs {
            match glyph {
                FrameGlyph::Border {
                    x,
                    y,
                    width,
                    height,
                    color,
                } => {
                    self.add_rect(&mut cursor_vertices, *x, *y, *width, *height, color);
                }
                FrameGlyph::ScrollBar {
                    horizontal,
                    x,
                    y,
                    width,
                    height,
                    thumb_start,
                    thumb_size,
                    track_color,
                    thumb_color,
                } => {
                    // Draw scroll bar track (subtle, configurable opacity)
                    let subtle_track = Color::new(
                        track_color.r, track_color.g, track_color.b,
                        track_color.a * self.scroll_bar_track_opacity,
                    );
                    self.add_rect(&mut cursor_vertices, *x, *y, *width, *height, &subtle_track);

                    // Compute thumb bounds
                    let (tx, ty, tw, th) = if *horizontal {
                        (*x + *thumb_start, *y, *thumb_size, *height)
                    } else {
                        (*x, *y + *thumb_start, *width, *thumb_size)
                    };

                    // Check hover: brighten thumb if mouse is over the scroll bar area
                    let (mx, my) = mouse_pos;
                    let hovered = mx >= *x && mx <= *x + *width
                        && my >= *y && my <= *y + *height;
                    let bright = self.scroll_bar_hover_brightness;
                    let effective_thumb = if hovered {
                        Color::new(
                            (thumb_color.r * bright).min(1.0),
                            (thumb_color.g * bright).min(1.0),
                            (thumb_color.b * bright).min(1.0),
                            thumb_color.a.min(1.0),
                        )
                    } else {
                        *thumb_color
                    };

                    // Rounded thumb with configurable pill radius
                    let radius = tw.min(th) * self.scroll_bar_thumb_radius;
                    scroll_bar_thumb_vertices.push((tx, ty, tw, th, radius, effective_thumb));
                }
                FrameGlyph::Cursor {
                    window_id,
                    x,
                    y,
                    width,
                    height,
                    style,
                    color,
                } => {
                    // Compute effective cursor color (possibly overridden by color cycling)
                    let cycle_color;
                    let effective_color = if self.cursor_color_cycle_enabled && *style != 3 {
                        let elapsed = self.cursor_color_cycle_start.elapsed().as_secs_f32();
                        let hue = (elapsed * self.cursor_color_cycle_speed) % 1.0;
                        cycle_color = Self::hsl_to_color(hue, self.cursor_color_cycle_saturation, self.cursor_color_cycle_lightness);
                        self.needs_continuous_redraw = true;
                        &cycle_color
                    } else {
                        color
                    };
                    if *style == 0 {
                        // Filled box cursor: split into bg rect + behind-text trail.
                        // The static cursor bg rect uses cursor_inverse info if available,
                        // otherwise falls back to the cursor color at the static position.
                        if cursor_visible {
                            if let Some(ref inv) = frame_glyphs.cursor_inverse {
                                // Draw cursor bg rect at static position (inverse video background)
                                let inv_color = if self.cursor_color_cycle_enabled {
                                    effective_color
                                } else {
                                    &inv.cursor_bg
                                };
                                self.add_rect(&mut cursor_bg_vertices,
                                    inv.x, inv.y, inv.width, inv.height, inv_color);
                            } else {
                                // No inverse info — draw opaque cursor at static position
                                self.add_rect(&mut cursor_bg_vertices, *x, *y, *width, *height, effective_color);
                            }

                            // Draw animated trail/rect behind text
                            let use_corners = if let Some(ref anim) = animated_cursor {
                                *window_id == anim.window_id && anim.corners.is_some()
                            } else {
                                false
                            };

                            if use_corners {
                                let anim = animated_cursor.as_ref().unwrap();
                                let corners = anim.corners.as_ref().unwrap();
                                self.add_quad(&mut behind_text_cursor_vertices, corners, effective_color);
                            } else if let Some(ref anim) = animated_cursor {
                                if *window_id == anim.window_id {
                                    self.add_rect(&mut behind_text_cursor_vertices,
                                        anim.x, anim.y, anim.width, anim.height, effective_color);
                                }
                            }
                        }
                    } else {
                        // Non-filled-box cursors: bar, hbar, hollow — drawn ON TOP of text
                        let use_corners = if let Some(ref anim) = animated_cursor {
                            *window_id == anim.window_id && *style != 3 && anim.corners.is_some()
                        } else {
                            false
                        };

                        if use_corners {
                            let anim = animated_cursor.as_ref().unwrap();
                            let corners = anim.corners.as_ref().unwrap();
                            if cursor_visible {
                                self.add_quad(&mut cursor_vertices, corners, effective_color);
                            }
                        } else {
                            let (cx, cy, cw, ch) = if let Some(ref anim) = animated_cursor {
                                if *window_id == anim.window_id && *style != 3 {
                                    (anim.x, anim.y, anim.width, anim.height)
                                } else {
                                    (*x, *y, *width, *height)
                                }
                            } else {
                                (*x, *y, *width, *height)
                            };

                            let should_draw = *style == 3 || cursor_visible;
                            if should_draw {
                                match style {
                                    1 => {
                                        // Bar (thin vertical line)
                                        self.add_rect(&mut cursor_vertices, cx, cy, 2.0, ch, effective_color);
                                    }
                                    2 => {
                                        // Underline (hbar at bottom)
                                        self.add_rect(&mut cursor_vertices, cx, cy + ch - 2.0, cw, 2.0, effective_color);
                                    }
                                    3 => {
                                        // Hollow box (4 border edges)
                                        self.add_rect(&mut cursor_vertices, cx, cy, cw, 1.0, effective_color);
                                        self.add_rect(&mut cursor_vertices, cx, cy + ch - 1.0, cw, 1.0, effective_color);
                                        self.add_rect(&mut cursor_vertices, cx, cy, 1.0, ch, effective_color);
                                        self.add_rect(&mut cursor_vertices, cx + cw - 1.0, cy, 1.0, ch, effective_color);
                                    }
                                    _ => {
                                        self.add_rect(&mut cursor_vertices, cx, cy, cw, ch, effective_color);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Glyphs Encoder"),
            });

        // Render pass - Clear with frame background color since we rebuild
        // the entire frame from current_matrix each time (no incremental updates).
        let bg = &frame_glyphs.background;
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Frame Glyphs Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            // Pre-multiply RGB by alpha for correct compositing
                            r: (bg.r * bg.a) as f64,
                            g: (bg.g * bg.a) as f64,
                            b: (bg.b * bg.a) as f64,
                            a: bg.a as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // === Step 1: Draw non-overlay backgrounds ===
            if !non_overlay_rect_vertices.is_empty() {
                let rect_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Non-overlay Rect Buffer"),
                            contents: bytemuck::cast_slice(&non_overlay_rect_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, rect_buffer.slice(..));
                render_pass.draw(0..non_overlay_rect_vertices.len() as u32, 0..1);
            }

            // === Step 1a: Background pattern (dots/grid/crosshatch) ===
            if self.bg_pattern_style > 0 {
                let spacing = self.bg_pattern_spacing.max(4.0);
                let (pr, pg, pb) = self.bg_pattern_color;
                let alpha = self.bg_pattern_opacity;
                let pat_color = Color::new(pr, pg, pb, alpha);
                let frame_w = frame_glyphs.width;
                let frame_h = frame_glyphs.height;
                let mut pattern_vertices: Vec<RectVertex> = Vec::new();

                match self.bg_pattern_style {
                    1 => {
                        // Dots: small squares at grid intersections
                        let dot_size = 1.0;
                        let mut y = 0.0_f32;
                        while y < frame_h {
                            let mut x = 0.0_f32;
                            while x < frame_w {
                                self.add_rect(&mut pattern_vertices, x, y, dot_size, dot_size, &pat_color);
                                x += spacing;
                            }
                            y += spacing;
                        }
                    }
                    2 => {
                        // Grid: horizontal and vertical lines
                        let line_w = 1.0;
                        let mut y = 0.0_f32;
                        while y < frame_h {
                            self.add_rect(&mut pattern_vertices, 0.0, y, frame_w, line_w, &pat_color);
                            y += spacing;
                        }
                        let mut x = 0.0_f32;
                        while x < frame_w {
                            self.add_rect(&mut pattern_vertices, x, 0.0, line_w, frame_h, &pat_color);
                            x += spacing;
                        }
                    }
                    3 => {
                        // Crosshatch: diagonal lines (approximated with small segments)
                        let line_w = 1.0;
                        let step = 2.0;
                        let diag_spacing = spacing * 1.414; // sqrt(2) for diagonal
                        // Forward diagonals (top-left to bottom-right)
                        let mut offset = -frame_h;
                        while offset < frame_w {
                            let mut t = 0.0_f32;
                            while t < frame_h.min(frame_w) {
                                let px = offset + t;
                                let py = t;
                                if px >= 0.0 && px < frame_w && py >= 0.0 && py < frame_h {
                                    self.add_rect(&mut pattern_vertices, px, py, line_w, step, &pat_color);
                                }
                                t += step;
                            }
                            offset += diag_spacing;
                        }
                        // Back diagonals (top-right to bottom-left)
                        let mut offset = 0.0_f32;
                        while offset < frame_w + frame_h {
                            let mut t = 0.0_f32;
                            while t < frame_h.min(frame_w) {
                                let px = offset - t;
                                let py = t;
                                if px >= 0.0 && px < frame_w && py >= 0.0 && py < frame_h {
                                    self.add_rect(&mut pattern_vertices, px, py, line_w, step, &pat_color);
                                }
                                t += step;
                            }
                            offset += diag_spacing;
                        }
                    }
                    _ => {}
                }

                if !pattern_vertices.is_empty() {
                    let pattern_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Background Pattern Buffer"),
                            contents: bytemuck::cast_slice(&pattern_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, pattern_buffer.slice(..));
                    render_pass.draw(0..pattern_vertices.len() as u32, 0..1);
                }
            }

            // === Step 1b: Draw filled rounded rect backgrounds for ROUNDED boxed spans ===
            // Only for corner_radius > 0. Standard boxes use normal rect backgrounds.
            {
                let mut box_fill_vertices: Vec<RoundedRectVertex> = Vec::new();
                for span in &box_spans {
                    if span.is_overlay { continue; }
                    if let Some(ref bg_color) = span.bg {
                        if let Some(face) = faces.get(&span.face_id) {
                            if face.box_corner_radius <= 0 { continue; }
                            let radius = (face.box_corner_radius as f32)
                                .min(span.height * 0.45)
                                .min(span.width * 0.45);
                            // Use a border_width larger than half the rect to fill solid
                            let fill_bw = span.height.max(span.width);
                            self.add_rounded_rect(
                                &mut box_fill_vertices,
                                span.x, span.y, span.width, span.height,
                                fill_bw, radius, bg_color,
                            );
                        }
                    }
                }
                if !box_fill_vertices.is_empty() {
                    let fill_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Box Fill Buffer"),
                            contents: bytemuck::cast_slice(&box_fill_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rounded_rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, fill_buffer.slice(..));
                    render_pass.draw(0..box_fill_vertices.len() as u32, 0..1);
                }
            }

            // === Step 1c: Draw cursor glow effect (behind cursor and text) ===
            if self.cursor_glow_enabled && cursor_visible {
                // Find active cursor position (style != 3 = not hollow/inactive)
                let mut glow_pos: Option<(f32, f32, f32, f32)> = None;
                if let Some(ref anim) = animated_cursor {
                    glow_pos = Some((anim.x, anim.y, anim.width, anim.height));
                } else {
                    for glyph in &frame_glyphs.glyphs {
                        if let FrameGlyph::Cursor { x, y, width, height, style, .. } = glyph {
                            if *style != 3 {
                                glow_pos = Some((*x, *y, *width, *height));
                                break;
                            }
                        }
                    }
                }

                if let Some((cx, cy, cw, ch)) = glow_pos {
                    let (gr, gg, gb) = self.cursor_glow_color;
                    let radius = self.cursor_glow_radius;
                    let mut peak_alpha = self.cursor_glow_opacity;

                    // Apply pulse modulation if enabled
                    if self.cursor_pulse_enabled {
                        let elapsed = self.cursor_pulse_start.elapsed().as_secs_f32();
                        let phase = elapsed * self.cursor_pulse_speed * 2.0 * std::f32::consts::PI;
                        // Sine wave: maps [min_opacity..1.0] range
                        let t = (phase.sin() + 1.0) / 2.0; // 0.0 to 1.0
                        let factor = self.cursor_pulse_min_opacity + t * (1.0 - self.cursor_pulse_min_opacity);
                        peak_alpha *= factor;
                    }
                    let layers = (radius / 2.0).ceil() as i32;
                    let mut glow_vertices: Vec<RectVertex> = Vec::new();

                    // Center of cursor
                    let center_x = cx + cw / 2.0;
                    let center_y = cy + ch / 2.0;

                    for i in 0..layers {
                        let t = (i + 1) as f32 / layers as f32;
                        let r = radius * t;
                        let alpha = peak_alpha * (1.0 - t * t); // quadratic falloff
                        let c = Color::new(gr, gg, gb, alpha);
                        let gx = center_x - r;
                        let gy = center_y - r;
                        let gw = r * 2.0;
                        let gh = r * 2.0;
                        self.add_rect(&mut glow_vertices, gx, gy, gw, gh, &c);
                    }

                    if !glow_vertices.is_empty() {
                        let glow_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Cursor Glow Buffer"),
                                contents: bytemuck::cast_slice(&glow_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, glow_buffer.slice(..));
                        render_pass.draw(0..glow_vertices.len() as u32, 0..1);
                    }
                }
            }

            // === Step 2: Draw cursor bg rect (inverse video background) ===
            // Drawn after window/char backgrounds but before text, so the cursor
            // background color is visible behind the inverse-video character.
            if !cursor_bg_vertices.is_empty() {
                let cursor_bg_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Cursor BG Rect Buffer"),
                            contents: bytemuck::cast_slice(&cursor_bg_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, cursor_bg_buffer.slice(..));
                render_pass.draw(0..cursor_bg_vertices.len() as u32, 0..1);
            }

            // === Step 3: Draw animated cursor trail behind text ===
            // The spring trail or animated rect for filled box cursor appears
            // behind text so characters remain readable during cursor motion.
            if !behind_text_cursor_vertices.is_empty() {
                let trail_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Behind-Text Cursor Buffer"),
                            contents: bytemuck::cast_slice(&behind_text_cursor_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, trail_buffer.slice(..));
                render_pass.draw(0..behind_text_cursor_vertices.len() as u32, 0..1);
            }

            // === Steps 4-6: Draw text and overlay in correct z-order ===
            // For each overlay pass:
            //   Pass 0 (non-overlay): draw buffer text (with cursor fg swap for inverse video)
            //   Pass 1 (overlay): draw overlay backgrounds first, then overlay text
            //
            // This ensures: non-overlay bg → cursor bg → trail → text → overlay bg → overlay text

            for overlay_pass in 0..2 {
                let want_overlay = overlay_pass == 1;

                // === Step 3: Draw overlay backgrounds before overlay text ===
                if want_overlay && !overlay_rect_vertices.is_empty() {
                    let rect_buffer =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Overlay Rect Buffer"),
                                contents: bytemuck::cast_slice(&overlay_rect_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            });

                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, rect_buffer.slice(..));
                    render_pass.draw(0..overlay_rect_vertices.len() as u32, 0..1);
                }

                // Draw filled rounded rect backgrounds for overlay ROUNDED boxed spans.
                if want_overlay {
                    let mut overlay_box_fill: Vec<RoundedRectVertex> = Vec::new();
                    for span in &box_spans {
                        if !span.is_overlay {
                            continue;
                        }
                        if let Some(ref bg_color) = span.bg {
                            if let Some(face) = faces.get(&span.face_id) {
                                if face.box_corner_radius <= 0 { continue; }
                                let radius = (face.box_corner_radius as f32)
                                    .min(span.height * 0.45)
                                    .min(span.width * 0.45);
                                let fill_bw = span.height.max(span.width);
                                self.add_rounded_rect(
                                    &mut overlay_box_fill,
                                    span.x, span.y, span.width, span.height,
                                    fill_bw, radius, bg_color,
                                );
                            }
                        }
                    }
                    if !overlay_box_fill.is_empty() {
                        let fill_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Overlay Box Fill Buffer"),
                                contents: bytemuck::cast_slice(&overlay_box_fill),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rounded_rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, fill_buffer.slice(..));
                        render_pass.draw(0..overlay_box_fill.len() as u32, 0..1);
                    }
                }

                let mut mask_data: Vec<(GlyphKey, [GlyphVertex; 6])> = Vec::new();
                let mut color_data: Vec<(GlyphKey, [GlyphVertex; 6])> = Vec::new();

                for glyph in &frame_glyphs.glyphs {
                    if let FrameGlyph::Char { char, x, y, width, ascent, fg, face_id, font_size, is_overlay, .. } = glyph {
                        if *is_overlay != want_overlay {
                            continue;
                        }

                        let key = GlyphKey {
                            charcode: *char as u32,
                            face_id: *face_id,
                            font_size_bits: font_size.to_bits(),
                        };

                        let face = faces.get(face_id);

                        if let Some(cached) = glyph_atlas.get_or_create(&self.device, &self.queue, &key, face) {
                            // Cached glyphs are rasterized at physical resolution (scale_factor).
                            // Divide bearing/size by scale_factor to get logical pixel positions
                            // that match Emacs coordinate space.
                            let sf = self.scale_factor;
                            let ya = if has_line_anims { *y + self.line_y_offset(*x, *y) } else { *y };
                            let glyph_x = *x + cached.bearing_x / sf;
                            let baseline = ya + *ascent;
                            let glyph_y = baseline - cached.bearing_y / sf;
                            let glyph_w = cached.width as f32 / sf;
                            let glyph_h = cached.height as f32 / sf;

                            // Determine effective foreground color.
                            // For the character under a filled box cursor, swap to
                            // cursor_fg (inverse video) when cursor is visible.
                            let effective_fg = if cursor_visible {
                                if let Some(ref inv) = frame_glyphs.cursor_inverse {
                                    // Match if char cell overlaps cursor inverse position
                                    if (*x - inv.x).abs() < 1.0 && (*y - inv.y).abs() < 1.0 {
                                        &inv.cursor_fg
                                    } else {
                                        fg
                                    }
                                } else {
                                    fg
                                }
                            } else {
                                fg
                            };

                            // Color glyphs use white vertex color (no tinting),
                            // mask glyphs use foreground color for tinting
                            let color = if cached.is_color {
                                [1.0, 1.0, 1.0, 1.0]
                            } else {
                                [effective_fg.r, effective_fg.g, effective_fg.b, effective_fg.a]
                            };

                            let vertices = [
                                GlyphVertex { position: [glyph_x, glyph_y], tex_coords: [0.0, 0.0], color },
                                GlyphVertex { position: [glyph_x + glyph_w, glyph_y], tex_coords: [1.0, 0.0], color },
                                GlyphVertex { position: [glyph_x + glyph_w, glyph_y + glyph_h], tex_coords: [1.0, 1.0], color },
                                GlyphVertex { position: [glyph_x, glyph_y], tex_coords: [0.0, 0.0], color },
                                GlyphVertex { position: [glyph_x + glyph_w, glyph_y + glyph_h], tex_coords: [1.0, 1.0], color },
                                GlyphVertex { position: [glyph_x, glyph_y + glyph_h], tex_coords: [0.0, 1.0], color },
                            ];

                            if cached.is_color {
                                color_data.push((key, vertices));
                            } else {
                                mask_data.push((key, vertices));
                            }
                        }
                    }
                }

                log::trace!("render_frame_glyphs: overlay={} {} mask glyphs, {} color glyphs",
                    want_overlay, mask_data.len(), color_data.len());

                // Draw mask glyphs with glyph pipeline (alpha tinted with foreground)
                // Sort by GlyphKey so identical characters batch into single draw calls,
                // significantly reducing GPU state changes (set_bind_group calls).
                if !mask_data.is_empty() {
                    mask_data.sort_by(|(a, _), (b, _)| {
                        a.face_id.cmp(&b.face_id)
                            .then(a.font_size_bits.cmp(&b.font_size_bits))
                            .then(a.charcode.cmp(&b.charcode))
                    });

                    render_pass.set_pipeline(&self.glyph_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

                    let all_vertices: Vec<GlyphVertex> = mask_data.iter()
                        .flat_map(|(_, verts)| verts.iter().copied())
                        .collect();

                    let glyph_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Glyph Vertex Buffer"),
                        contents: bytemuck::cast_slice(&all_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

                    render_pass.set_vertex_buffer(0, glyph_buffer.slice(..));

                    // Batch consecutive glyphs sharing the same texture
                    let mut i = 0;
                    while i < mask_data.len() {
                        let (ref key, _) = mask_data[i];
                        if let Some(cached) = glyph_atlas.get(key) {
                            let batch_start = i;
                            i += 1;
                            while i < mask_data.len() && mask_data[i].0 == *key {
                                i += 1;
                            }
                            let vert_start = (batch_start * 6) as u32;
                            let vert_end = (i * 6) as u32;
                            render_pass.set_bind_group(1, &cached.bind_group, &[]);
                            render_pass.draw(vert_start..vert_end, 0..1);
                        } else {
                            i += 1;
                        }
                    }
                }

                // Draw color glyphs with image pipeline (direct RGBA, e.g. color emoji)
                if !color_data.is_empty() {
                    color_data.sort_by(|(a, _), (b, _)| {
                        a.face_id.cmp(&b.face_id)
                            .then(a.font_size_bits.cmp(&b.font_size_bits))
                            .then(a.charcode.cmp(&b.charcode))
                    });

                    render_pass.set_pipeline(&self.image_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

                    let all_vertices: Vec<GlyphVertex> = color_data.iter()
                        .flat_map(|(_, verts)| verts.iter().copied())
                        .collect();

                    let color_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Color Glyph Vertex Buffer"),
                        contents: bytemuck::cast_slice(&all_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

                    render_pass.set_vertex_buffer(0, color_buffer.slice(..));

                    // Batch consecutive color glyphs sharing the same texture
                    let mut i = 0;
                    while i < color_data.len() {
                        let (ref key, _) = color_data[i];
                        if let Some(cached) = glyph_atlas.get(key) {
                            let batch_start = i;
                            i += 1;
                            while i < color_data.len() && color_data[i].0 == *key {
                                i += 1;
                            }
                            let vert_start = (batch_start * 6) as u32;
                            let vert_end = (i * 6) as u32;
                            render_pass.set_bind_group(1, &cached.bind_group, &[]);
                            render_pass.draw(vert_start..vert_end, 0..1);
                        } else {
                            i += 1;
                        }
                    }
                }

                // === Draw text decorations (underline, overline, strike-through) ===
                // Rendered after text so decorations appear on top of glyphs.
                // Box borders are handled separately via merged box_spans below.
                {
                    let mut decoration_vertices: Vec<RectVertex> = Vec::new();

                    for glyph in &frame_glyphs.glyphs {
                        if let FrameGlyph::Char {
                            x, y, width, height, ascent, fg,
                            face_id,
                            underline, underline_color,
                            strike_through, strike_through_color,
                            overline, overline_color,
                            is_overlay, ..
                        } = glyph {
                            if *is_overlay != want_overlay {
                                continue;
                            }

                            let ya = if has_line_anims { *y + self.line_y_offset(*x, *y) } else { *y };
                            let baseline_y = ya + *ascent;

                            // Get per-face font metrics for proper decoration positioning
                            let (ul_pos, ul_thick) = frame_glyphs.faces.get(face_id)
                                .map(|f| (f.underline_position as f32, f.underline_thickness as f32))
                                .unwrap_or((1.0, 1.0));

                            // --- Underline ---
                            if *underline > 0 {
                                let ul_color = underline_color.as_ref().unwrap_or(fg);
                                let ul_y = baseline_y + ul_pos;
                                let line_thickness = ul_thick.max(1.0);

                                match underline {
                                    1 => {
                                        // Single solid line
                                        self.add_rect(&mut decoration_vertices, *x, ul_y, *width, line_thickness, ul_color);
                                    }
                                    2 => {
                                        // Wave: smooth sine wave underline
                                        let amplitude: f32 = 2.0;
                                        let wavelength: f32 = 8.0;
                                        let seg_w: f32 = 1.0;
                                        let mut cx = *x;
                                        while cx < *x + *width {
                                            let sw = seg_w.min(*x + *width - cx);
                                            let phase = (cx - *x) * std::f32::consts::TAU / wavelength;
                                            let offset = phase.sin() * amplitude;
                                            self.add_rect(&mut decoration_vertices, cx, ul_y + offset, sw, line_thickness, ul_color);
                                            cx += seg_w;
                                        }
                                    }
                                    3 => {
                                        // Double line
                                        self.add_rect(&mut decoration_vertices, *x, ul_y, *width, line_thickness, ul_color);
                                        self.add_rect(&mut decoration_vertices, *x, ul_y + line_thickness + 1.0, *width, line_thickness, ul_color);
                                    }
                                    4 => {
                                        // Dots (dot size = thickness, gap = 2px)
                                        let mut cx = *x;
                                        while cx < *x + *width {
                                            let dw = line_thickness.min(*x + *width - cx);
                                            self.add_rect(&mut decoration_vertices, cx, ul_y, dw, line_thickness, ul_color);
                                            cx += line_thickness + 2.0;
                                        }
                                    }
                                    5 => {
                                        // Dashes (4px with 3px gap)
                                        let mut cx = *x;
                                        while cx < *x + *width {
                                            let dw = 4.0_f32.min(*x + *width - cx);
                                            self.add_rect(&mut decoration_vertices, cx, ul_y, dw, line_thickness, ul_color);
                                            cx += 7.0;
                                        }
                                    }
                                    _ => {
                                        // Fallback: single line
                                        self.add_rect(&mut decoration_vertices, *x, ul_y, *width, line_thickness, ul_color);
                                    }
                                }
                            }

                            // --- Overline ---
                            if *overline > 0 {
                                let ol_color = overline_color.as_ref().unwrap_or(fg);
                                self.add_rect(&mut decoration_vertices, *x, ya, *width, ul_thick.max(1.0), ol_color);
                            }

                            // --- Strike-through ---
                            if *strike_through > 0 {
                                let st_color = strike_through_color.as_ref().unwrap_or(fg);
                                // Position at ~1/3 of ascent above baseline (standard typographic position)
                                let st_y = baseline_y - *ascent / 3.0;
                                self.add_rect(&mut decoration_vertices, *x, st_y, *width, ul_thick.max(1.0), st_color);
                            }
                        }
                    }

                    if !decoration_vertices.is_empty() {
                        let decoration_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Decoration Rect Buffer"),
                            contents: bytemuck::cast_slice(&decoration_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, decoration_buffer.slice(..));
                        render_pass.draw(0..decoration_vertices.len() as u32, 0..1);
                    }
                }

                // === Draw box borders (merged spans) ===
                // Standard boxes (corner_radius=0): merged rect borders (top/bottom/left/right).
                // Rounded boxes (corner_radius>0): SDF border ring.
                {
                    // Sharp box borders as merged rect spans
                    let mut sharp_border_vertices: Vec<RectVertex> = Vec::new();
                    // Rounded box borders via SDF
                    let mut rounded_border_vertices: Vec<RoundedRectVertex> = Vec::new();

                    // Filter spans for this overlay pass
                    let pass_spans: Vec<usize> = box_spans.iter().enumerate()
                        .filter(|(_, s)| s.is_overlay == want_overlay)
                        .map(|(i, _)| i)
                        .collect();

                    for (idx_in_pass, &span_idx) in pass_spans.iter().enumerate() {
                        let span = &box_spans[span_idx];
                        if let Some(face) = faces.get(&span.face_id) {
                            let bx_color = face.box_color.as_ref().unwrap_or(&face.foreground);
                            let bw = face.box_line_width as f32;

                            if face.box_corner_radius > 0 {
                                // Rounded border via SDF
                                let radius = (face.box_corner_radius as f32)
                                    .min(span.height * 0.45)
                                    .min(span.width * 0.45);
                                self.add_rounded_rect(
                                    &mut rounded_border_vertices,
                                    span.x, span.y, span.width, span.height,
                                    bw, radius, bx_color,
                                );
                            } else {
                                // Sharp border — for overlay spans (mode-line), suppress
                                // internal left/right borders between adjacent spans for
                                // continuity. For non-overlay spans, always draw all 4 borders.
                                let suppress_internal = span.is_overlay;
                                let has_left_neighbor = suppress_internal && idx_in_pass > 0 && {
                                    let prev = &box_spans[pass_spans[idx_in_pass - 1]];
                                    (prev.y - span.y).abs() < 0.5
                                        && ((prev.x + prev.width) - span.x).abs() < 1.5
                                };
                                let has_right_neighbor = suppress_internal && idx_in_pass + 1 < pass_spans.len() && {
                                    let next = &box_spans[pass_spans[idx_in_pass + 1]];
                                    (next.y - span.y).abs() < 0.5
                                        && (next.x - (span.x + span.width)).abs() < 1.5
                                };

                                // Compute edge colors for 3D box types
                                let (top_left_color, bottom_right_color) = match face.box_type {
                                    BoxType::Raised3D => {
                                        let light = Color {
                                            r: (bx_color.r * 1.4).min(1.0),
                                            g: (bx_color.g * 1.4).min(1.0),
                                            b: (bx_color.b * 1.4).min(1.0),
                                            a: bx_color.a,
                                        };
                                        let dark = Color {
                                            r: bx_color.r * 0.6,
                                            g: bx_color.g * 0.6,
                                            b: bx_color.b * 0.6,
                                            a: bx_color.a,
                                        };
                                        (light, dark)
                                    }
                                    BoxType::Sunken3D => {
                                        let light = Color {
                                            r: (bx_color.r * 1.4).min(1.0),
                                            g: (bx_color.g * 1.4).min(1.0),
                                            b: (bx_color.b * 1.4).min(1.0),
                                            a: bx_color.a,
                                        };
                                        let dark = Color {
                                            r: bx_color.r * 0.6,
                                            g: bx_color.g * 0.6,
                                            b: bx_color.b * 0.6,
                                            a: bx_color.a,
                                        };
                                        (dark, light)
                                    }
                                    _ => (bx_color.clone(), bx_color.clone()),
                                };

                                // Top
                                self.add_rect(&mut sharp_border_vertices, span.x, span.y, span.width, bw, &top_left_color);
                                // Bottom
                                self.add_rect(&mut sharp_border_vertices, span.x, span.y + span.height - bw, span.width, bw, &bottom_right_color);
                                // Left (only if no adjacent span to the left on same row)
                                if !has_left_neighbor {
                                    self.add_rect(&mut sharp_border_vertices, span.x, span.y, bw, span.height, &top_left_color);
                                }
                                // Right (only if no adjacent span to the right on same row)
                                if !has_right_neighbor {
                                    self.add_rect(&mut sharp_border_vertices, span.x + span.width - bw, span.y, bw, span.height, &bottom_right_color);
                                }
                            }
                        }
                    }

                    // Draw sharp box borders
                    if !sharp_border_vertices.is_empty() {
                        let sharp_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Sharp Box Border Buffer"),
                                contents: bytemuck::cast_slice(&sharp_border_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, sharp_buffer.slice(..));
                        render_pass.draw(0..sharp_border_vertices.len() as u32, 0..1);
                    }

                    // Draw rounded box borders
                    if !rounded_border_vertices.is_empty() {
                        let rounded_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Rounded Box Border Buffer"),
                                contents: bytemuck::cast_slice(&rounded_border_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rounded_rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, rounded_buffer.slice(..));
                        render_pass.draw(0..rounded_border_vertices.len() as u32, 0..1);
                    }
                }
            }

            // Draw inline images
            render_pass.set_pipeline(&self.image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Image { image_id, x, y, width, height } = glyph {
                    // Clip to mode-line boundary if needed
                    let (clipped_height, tex_v_max) = if let Some(oy) = overlay_y {
                        if *y + *height > oy {
                            let clipped = (oy - *y).max(0.0);
                            let v_max = if *height > 0.0 { clipped / *height } else { 1.0 };
                            (clipped, v_max)
                        } else {
                            (*height, 1.0)
                        }
                    } else {
                        (*height, 1.0)
                    };

                    // Skip if fully clipped
                    if clipped_height <= 0.0 {
                        continue;
                    }

                    log::debug!("Rendering image {} at ({}, {}) size {}x{} (clipped to {})",
                        image_id, x, y, width, height, clipped_height);
                    // Check if image texture is ready
                    if let Some(cached) = self.image_cache.get(*image_id) {
                        // Create vertices for image quad (white color = no tinting)
                        let vertices = [
                            GlyphVertex { position: [*x, *y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [*x + *width, *y], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [*x + *width, *y + clipped_height], tex_coords: [1.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [*x, *y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [*x + *width, *y + clipped_height], tex_coords: [1.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [*x, *y + clipped_height], tex_coords: [0.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                        ];

                        let image_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Image Vertex Buffer"),
                            contents: bytemuck::cast_slice(&vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                        render_pass.set_bind_group(1, &cached.bind_group, &[]);
                        render_pass.set_vertex_buffer(0, image_buffer.slice(..));
                        render_pass.draw(0..6, 0..1);
                    }
                }
            }

            // Draw inline videos
            #[cfg(feature = "video")]
            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Video { video_id, x, y, width, height } = glyph {
                    // Clip to mode-line boundary if needed
                    let (clipped_height, tex_v_max) = if let Some(oy) = overlay_y {
                        if *y + *height > oy {
                            let clipped = (oy - *y).max(0.0);
                            let v_max = if *height > 0.0 { clipped / *height } else { 1.0 };
                            (clipped, v_max)
                        } else {
                            (*height, 1.0)
                        }
                    } else {
                        (*height, 1.0)
                    };

                    // Skip if fully clipped
                    if clipped_height <= 0.0 {
                        continue;
                    }

                    // Check if video texture is ready
                    if let Some(cached) = self.video_cache.get(*video_id) {
                        log::trace!("Rendering video {} at ({}, {}) size {}x{} (clipped to {}), frame_count={}",
                            video_id, x, y, width, height, clipped_height, cached.frame_count);
                        if let Some(ref bind_group) = cached.bind_group {
                            // Create vertices for video quad (white color = no tinting)
                            let vertices = [
                                GlyphVertex { position: [*x, *y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x + *width, *y], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x + *width, *y + clipped_height], tex_coords: [1.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x, *y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x + *width, *y + clipped_height], tex_coords: [1.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x, *y + clipped_height], tex_coords: [0.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                            ];

                            let video_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Video Vertex Buffer"),
                                contents: bytemuck::cast_slice(&vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            });

                            render_pass.set_bind_group(1, bind_group, &[]);
                            render_pass.set_vertex_buffer(0, video_buffer.slice(..));
                            render_pass.draw(0..6, 0..1);
                        } else {
                            log::warn!("Video {} has no bind_group!", video_id);
                        }
                    } else {
                        log::warn!("Video {} not found in cache!", video_id);
                    }
                }
            }

            // Draw inline webkit views (use opaque pipeline — DMA-BUF XRGB has alpha=0)
            #[cfg(feature = "wpe-webkit")]
            {
                render_pass.set_pipeline(&self.opaque_image_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

                for glyph in &frame_glyphs.glyphs {
                    if let FrameGlyph::WebKit { webkit_id, x, y, width, height } = glyph {
                        // Clip to mode-line boundary if needed
                        log::trace!("WebKit clip check: webkit {} at y={}, height={}, y+h={}, overlay_y={:?}",
                            webkit_id, y, height, y + height, overlay_y);
                        let (clipped_height, tex_v_max) = if let Some(oy) = overlay_y {
                            if *y + *height > oy {
                                let clipped = (oy - *y).max(0.0);
                                let v_max = if *height > 0.0 { clipped / *height } else { 1.0 };
                                log::trace!("WebKit {} clipped: y={} + h={} > overlay_y={}, clipped_height={}",
                                    webkit_id, y, height, oy, clipped);
                                (clipped, v_max)
                            } else {
                                (*height, 1.0)
                            }
                        } else {
                            (*height, 1.0)
                        };

                        // Skip if fully clipped
                        if clipped_height <= 0.0 {
                            continue;
                        }

                        // Check if webkit texture is ready
                        if let Some(cached) = self.webkit_cache.get(*webkit_id) {
                            log::debug!("Rendering webkit {} at ({}, {}) size {}x{} (clipped to {})",
                                webkit_id, x, y, width, height, clipped_height);
                            // Create vertices for webkit quad (white color = no tinting)
                            let vertices = [
                                GlyphVertex { position: [*x, *y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x + *width, *y], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x + *width, *y + clipped_height], tex_coords: [1.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x, *y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x + *width, *y + clipped_height], tex_coords: [1.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                                GlyphVertex { position: [*x, *y + clipped_height], tex_coords: [0.0, tex_v_max], color: [1.0, 1.0, 1.0, 1.0] },
                            ];

                            let webkit_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("WebKit Vertex Buffer"),
                                contents: bytemuck::cast_slice(&vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            });

                            render_pass.set_bind_group(1, &cached.bind_group, &[]);
                            render_pass.set_vertex_buffer(0, webkit_buffer.slice(..));
                            render_pass.draw(0..6, 0..1);
                        } else {
                            log::debug!("WebKit {} not found in cache", webkit_id);
                        }
                    }
                }
            }

            // Draw cursors and borders (after text)
            if !cursor_vertices.is_empty() {
                let cursor_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Cursor Vertex Buffer"),
                            contents: bytemuck::cast_slice(&cursor_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, cursor_buffer.slice(..));
                render_pass.draw(0..cursor_vertices.len() as u32, 0..1);
            }

            // === Draw scroll bar thumbs as filled rounded rects ===
            if !scroll_bar_thumb_vertices.is_empty() {
                let mut rounded_verts: Vec<RoundedRectVertex> = Vec::new();
                for (tx, ty, tw, th, radius, color) in &scroll_bar_thumb_vertices {
                    // border_width = 0 triggers filled mode in the shader
                    self.add_rounded_rect(&mut rounded_verts, *tx, *ty, *tw, *th, 0.0, *radius, color);
                }
                let thumb_buffer = self.device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Scroll Bar Thumb Buffer"),
                        contents: bytemuck::cast_slice(&rounded_verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    },
                );
                render_pass.set_pipeline(&self.rounded_rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, thumb_buffer.slice(..));
                render_pass.draw(0..rounded_verts.len() as u32, 0..1);
            }

            // === Draw mode-line separators ===
            if self.mode_line_separator_style > 0 {
                let (cr, cg, cb) = self.mode_line_separator_color;
                let sep_h = self.mode_line_separator_height;
                let style = self.mode_line_separator_style;
                let mut sep_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.mode_line_height > 0.0 && !info.is_minibuffer {
                        let b = &info.bounds;
                        let sep_y = b.y + b.height - info.mode_line_height - sep_h;

                        match style {
                            1 => {
                                // Solid line
                                let c = Color::new(cr, cg, cb, 0.6);
                                self.add_rect(&mut sep_vertices, b.x, sep_y, b.width, 1.0, &c);
                            }
                            2 => {
                                // Shadow (gradient from opaque to transparent)
                                let layers = sep_h.ceil() as i32;
                                for i in 0..layers {
                                    let t = i as f32 / layers as f32;
                                    let alpha = 0.3 * (1.0 - t);
                                    let c = Color::new(cr, cg, cb, alpha);
                                    self.add_rect(&mut sep_vertices, b.x, sep_y + i as f32, b.width, 1.0, &c);
                                }
                            }
                            3 => {
                                // Gradient (solid to transparent, going upward)
                                let layers = sep_h.ceil() as i32;
                                for i in 0..layers {
                                    let t = (layers - 1 - i) as f32 / layers as f32;
                                    let alpha = 0.4 * t;
                                    let c = Color::new(cr, cg, cb, alpha);
                                    self.add_rect(&mut sep_vertices, b.x, sep_y + i as f32, b.width, 1.0, &c);
                                }
                            }
                            _ => {}
                        }
                    }
                }

                if !sep_vertices.is_empty() {
                    let sep_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Mode-line Separator Buffer"),
                            contents: bytemuck::cast_slice(&sep_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, sep_buffer.slice(..));
                    render_pass.draw(0..sep_vertices.len() as u32, 0..1);
                }
            }

            // === Buffer-local accent color strip ===
            if self.accent_strip_enabled {
                let strip_w = self.accent_strip_width.max(1.0);
                let mut strip_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;

                    // Derive color from file extension hash
                    let ext = info.buffer_file_name.rsplit('.').next().unwrap_or("");
                    let (r, g, b_col) = Self::extension_to_color(ext);
                    let strip_h = b.height - info.mode_line_height;
                    if strip_h <= 0.0 { continue; }

                    let c = Color::new(r, g, b_col, 0.8);
                    self.add_rect(&mut strip_vertices, b.x, b.y, strip_w, strip_h, &c);
                }

                if !strip_vertices.is_empty() {
                    let strip_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Accent Strip Buffer"),
                            contents: bytemuck::cast_slice(&strip_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, strip_buffer.slice(..));
                    render_pass.draw(0..strip_vertices.len() as u32, 0..1);
                }
            }

            // === Window padding gradient (inner edge shading for depth) ===
            if self.padding_gradient_enabled {
                let grad_w = self.padding_gradient_width.max(1.0);
                let peak_alpha = self.padding_gradient_opacity.clamp(0.0, 1.0);
                let (cr, cg, cb) = self.padding_gradient_color;
                let steps = (grad_w as i32).max(2);
                let mut grad_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;
                    let content_h = b.height - info.mode_line_height;
                    if content_h <= 0.0 { continue; }

                    let step_size = grad_w / steps as f32;
                    for i in 0..steps {
                        let t = i as f32 / steps as f32;
                        let alpha = peak_alpha * (1.0 - t) * (1.0 - t);
                        if alpha < 0.005 { continue; }
                        let offset = i as f32 * step_size;
                        let c = Color::new(cr, cg, cb, alpha);

                        // Top edge
                        self.add_rect(&mut grad_vertices, b.x, b.y + offset, b.width, step_size, &c);
                        // Bottom edge (above mode-line)
                        let bot_y = b.y + content_h - offset - step_size;
                        if bot_y > b.y {
                            self.add_rect(&mut grad_vertices, b.x, bot_y, b.width, step_size, &c);
                        }
                        // Left edge
                        self.add_rect(&mut grad_vertices, b.x + offset, b.y, step_size, content_h, &c);
                        // Right edge
                        let right_x = b.x + b.width - offset - step_size;
                        if right_x > b.x {
                            self.add_rect(&mut grad_vertices, right_x, b.y, step_size, content_h, &c);
                        }
                    }
                }

                if !grad_vertices.is_empty() {
                    let grad_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Padding Gradient Buffer"),
                            contents: bytemuck::cast_slice(&grad_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, grad_buffer.slice(..));
                    render_pass.draw(0..grad_vertices.len() as u32, 0..1);
                }
            }

            // === Smooth border color transition on focus ===
            if self.border_transition_enabled && frame_glyphs.window_infos.len() > 1 {
                let now = std::time::Instant::now();
                let (ar, ag, ab) = self.border_transition_active_color;
                let duration = self.border_transition_duration;

                // Detect selection change
                let mut new_selected: Option<i64> = None;
                for info in &frame_glyphs.window_infos {
                    if info.selected && !info.is_minibuffer {
                        new_selected = Some(info.window_id);
                        break;
                    }
                }
                if let Some(sel_id) = new_selected {
                    if self.prev_border_selected != 0 && sel_id != self.prev_border_selected {
                        // Old window: becoming inactive (fade out)
                        self.border_transitions.retain(|&(wid, _, _)| wid != self.prev_border_selected && wid != sel_id);
                        self.border_transitions.push((self.prev_border_selected, false, now));
                        // New window: becoming active (fade in)
                        self.border_transitions.push((sel_id, true, now));
                    }
                    self.prev_border_selected = sel_id;
                }

                // Clean up expired transitions
                self.border_transitions.retain(|&(_, _, start)| now.duration_since(start) < duration);

                let border_thickness = 2.0_f32;
                let mut border_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;
                    let content_h = b.height - info.mode_line_height;
                    if content_h <= 0.0 { continue; }

                    // Determine alpha: active=1.0, inactive=0.0, transitioning=interpolated
                    let alpha = if let Some(&(_, becoming_active, start)) = self.border_transitions.iter().find(|&&(wid, _, _)| wid == info.window_id) {
                        let t = (now.duration_since(start).as_secs_f32() / duration.as_secs_f32()).min(1.0);
                        let eased = t * (2.0 - t); // ease-out
                        if becoming_active { eased } else { 1.0 - eased }
                    } else if info.selected {
                        1.0_f32
                    } else {
                        0.0_f32
                    };

                    if alpha < 0.01 { continue; }

                    let c = Color::new(ar, ag, ab, alpha * 0.7);
                    // Top border
                    self.add_rect(&mut border_vertices, b.x, b.y, b.width, border_thickness, &c);
                    // Bottom border (above mode-line)
                    self.add_rect(&mut border_vertices, b.x, b.y + content_h - border_thickness, b.width, border_thickness, &c);
                    // Left border
                    self.add_rect(&mut border_vertices, b.x, b.y, border_thickness, content_h, &c);
                    // Right border
                    self.add_rect(&mut border_vertices, b.x + b.width - border_thickness, b.y, border_thickness, content_h, &c);
                }

                if !border_vertices.is_empty() {
                    let border_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Border Transition Buffer"),
                            contents: bytemuck::cast_slice(&border_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, border_buffer.slice(..));
                    render_pass.draw(0..border_vertices.len() as u32, 0..1);
                    self.needs_continuous_redraw = true;
                }
            }

            // === Frosted glass effect on mode-lines ===
            if self.frosted_glass_enabled {
                let frost_opacity = self.frosted_glass_opacity.clamp(0.0, 1.0);
                let blur_r = self.frosted_glass_blur.max(1.0);
                let mut frost_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.mode_line_height <= 0.0 || info.is_minibuffer {
                        continue;
                    }
                    let b = &info.bounds;
                    let ml_y = b.y + b.height - info.mode_line_height;
                    let ml_h = info.mode_line_height;

                    // Layer 1: Base frosted overlay (white, semi-transparent)
                    let base_color = Color::new(1.0, 1.0, 1.0, frost_opacity * 0.15);
                    self.add_rect(&mut frost_vertices, b.x, ml_y, b.width, ml_h, &base_color);

                    // Layer 2-5: Offset blur layers (semi-transparent white at offsets)
                    let offsets = [
                        (-blur_r, 0.0), (blur_r, 0.0),
                        (0.0, -blur_r * 0.5), (0.0, blur_r * 0.5),
                    ];
                    let layer_alpha = frost_opacity * 0.06;
                    for (dx, dy) in offsets {
                        let c = Color::new(1.0, 1.0, 1.0, layer_alpha);
                        // Clamp to mode-line bounds
                        let rx = (b.x + dx).max(b.x);
                        let ry = (ml_y + dy).max(ml_y);
                        let rw = b.width.min(b.x + b.width - rx);
                        let rh = ml_h.min(ml_y + ml_h - ry);
                        if rw > 0.0 && rh > 0.0 {
                            self.add_rect(&mut frost_vertices, rx, ry, rw, rh, &c);
                        }
                    }

                    // Layer 6: Grain/noise pattern (stipple with small rects)
                    let grain_size = 2.0_f32;
                    let grain_alpha = frost_opacity * 0.04;
                    let cols = (b.width / grain_size) as i32;
                    let rows = (ml_h / grain_size) as i32;
                    // Use a simple hash-based pseudo-random for grain pattern
                    for row in 0..rows {
                        for col in 0..cols {
                            // Simple hash: (row * 7919 + col * 104729) % 3 == 0
                            let hash = ((row as u64).wrapping_mul(7919) + (col as u64).wrapping_mul(104729)) % 5;
                            if hash == 0 {
                                let gx = b.x + col as f32 * grain_size;
                                let gy = ml_y + row as f32 * grain_size;
                                let c = Color::new(1.0, 1.0, 1.0, grain_alpha);
                                self.add_rect(&mut frost_vertices, gx, gy, grain_size, grain_size, &c);
                            }
                        }
                    }

                    // Top edge: bright line for glass edge highlight
                    let edge_c = Color::new(1.0, 1.0, 1.0, frost_opacity * 0.3);
                    self.add_rect(&mut frost_vertices, b.x, ml_y, b.width, 1.0, &edge_c);
                }

                if !frost_vertices.is_empty() {
                    let frost_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Frosted Glass Buffer"),
                            contents: bytemuck::cast_slice(&frost_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, frost_buffer.slice(..));
                    render_pass.draw(0..frost_vertices.len() as u32, 0..1);
                }
            }

            // === Noise/film grain texture overlay ===
            if self.noise_grain_enabled {
                let grain_size = self.noise_grain_size.max(1.0);
                let intensity = self.noise_grain_intensity.clamp(0.0, 1.0);
                let frame_w = surface_width as f32 / self.scale_factor;
                let frame_h = surface_height as f32 / self.scale_factor;
                let cols = (frame_w / grain_size) as i32;
                let rows = (frame_h / grain_size) as i32;
                let frame_seed = self.noise_grain_frame as u64;
                self.noise_grain_frame = self.noise_grain_frame.wrapping_add(1);

                let mut grain_vertices: Vec<RectVertex> = Vec::new();
                for row in 0..rows {
                    for col in 0..cols {
                        // Hash-based pseudo-random per grain cell, animated by frame
                        let hash = ((row as u64).wrapping_mul(7919)
                            .wrapping_add((col as u64).wrapping_mul(104729))
                            .wrapping_add(frame_seed.wrapping_mul(31337))) % 97;
                        if hash < 15 {
                            // Alternate black/white grains
                            let lum = if hash < 8 { 0.0 } else { 1.0 };
                            let alpha = intensity * (hash as f32 / 15.0) * 0.5;
                            let gx = col as f32 * grain_size;
                            let gy = row as f32 * grain_size;
                            let c = Color::new(lum, lum, lum, alpha);
                            self.add_rect(&mut grain_vertices, gx, gy, grain_size, grain_size, &c);
                        }
                    }
                }

                if !grain_vertices.is_empty() {
                    let grain_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Noise Grain Buffer"),
                            contents: bytemuck::cast_slice(&grain_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, grain_buffer.slice(..));
                    render_pass.draw(0..grain_vertices.len() as u32, 0..1);
                }
                self.needs_continuous_redraw = true;
            }

            // === Focus mode: dim lines outside current paragraph ===
            if self.focus_mode_enabled {
                // Find active cursor Y position (style != 3)
                let mut cursor_y: Option<f32> = None;
                let mut cursor_h: f32 = 0.0;
                if let Some(ref anim) = animated_cursor {
                    cursor_y = Some(anim.y);
                    cursor_h = anim.height;
                } else {
                    for glyph in &frame_glyphs.glyphs {
                        if let FrameGlyph::Cursor { y, height, style, .. } = glyph {
                            if *style != 3 {
                                cursor_y = Some(*y);
                                cursor_h = *height;
                                break;
                            }
                        }
                    }
                }

                if let Some(cy) = cursor_y {
                    // Find selected window bounds
                    let mut sel_bounds: Option<&Rect> = None;
                    for info in &frame_glyphs.window_infos {
                        if info.selected && !info.is_minibuffer {
                            sel_bounds = Some(&info.bounds);
                            break;
                        }
                    }

                    if let Some(bounds) = sel_bounds {
                        let char_h = frame_glyphs.char_height.max(1.0);

                        // Collect unique row Y positions within the selected window
                        let mut row_ys: Vec<(f32, f32, bool)> = Vec::new(); // (y, height, has_non_space)
                        let mut last_y: f32 = -9999.0;
                        let mut last_h: f32 = 0.0;
                        let mut has_non_space = false;

                        for glyph in &frame_glyphs.glyphs {
                            if let FrameGlyph::Char { x, y, height, char: ch, is_overlay, .. } = glyph {
                                if *is_overlay { continue; }
                                if *x < bounds.x || *x >= bounds.x + bounds.width { continue; }
                                if *y < bounds.y || *y >= bounds.y + bounds.height { continue; }
                                if (*y - last_y).abs() > 0.5 {
                                    if last_y > 0.0 {
                                        row_ys.push((last_y, last_h, has_non_space));
                                    }
                                    last_y = *y;
                                    last_h = *height;
                                    has_non_space = false;
                                }
                                if *ch != ' ' && *ch != '\t' && *ch != '\n' {
                                    has_non_space = true;
                                }
                            }
                        }
                        if last_y > 0.0 {
                            row_ys.push((last_y, last_h, has_non_space));
                        }

                        // Find paragraph boundaries: blank lines around cursor
                        let cursor_row_idx = row_ys.iter().position(|(y, _, _)| (*y - cy).abs() < cursor_h);

                        if let Some(cursor_idx) = cursor_row_idx {
                            // Search backward for paragraph start (blank line or window top)
                            let mut para_start_y = bounds.y;
                            for i in (0..cursor_idx).rev() {
                                if !row_ys[i].2 { // blank line
                                    para_start_y = row_ys[i].0 + row_ys[i].1;
                                    break;
                                }
                            }

                            // Search forward for paragraph end (blank line or window bottom)
                            let mut para_end_y = bounds.y + bounds.height;
                            for i in (cursor_idx + 1)..row_ys.len() {
                                if !row_ys[i].2 { // blank line
                                    para_end_y = row_ys[i].0;
                                    break;
                                }
                            }

                            // Draw dim overlays above and below the paragraph
                            let dim_color = Color::new(0.0, 0.0, 0.0, self.focus_mode_opacity);
                            let mut focus_vertices: Vec<RectVertex> = Vec::new();

                            // Above paragraph
                            if para_start_y > bounds.y + 1.0 {
                                self.add_rect(&mut focus_vertices,
                                    bounds.x, bounds.y, bounds.width,
                                    para_start_y - bounds.y, &dim_color);
                            }
                            // Below paragraph
                            if para_end_y < bounds.y + bounds.height - 1.0 {
                                self.add_rect(&mut focus_vertices,
                                    bounds.x, para_end_y, bounds.width,
                                    bounds.y + bounds.height - para_end_y, &dim_color);
                            }

                            if !focus_vertices.is_empty() {
                                let focus_buffer = self.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label: Some("Focus Mode Buffer"),
                                        contents: bytemuck::cast_slice(&focus_vertices),
                                        usage: wgpu::BufferUsages::VERTEX,
                                    },
                                );
                                render_pass.set_pipeline(&self.rect_pipeline);
                                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                                render_pass.set_vertex_buffer(0, focus_buffer.slice(..));
                                render_pass.draw(0..focus_vertices.len() as u32, 0..1);
                            }
                        }
                    }
                }
            }

            // === Draw inactive window dimming overlays (with smooth fade) ===
            if self.inactive_dim_enabled && frame_glyphs.window_infos.len() > 1 {
                let now = std::time::Instant::now();
                let dt = now.duration_since(self.last_dim_tick).as_secs_f32().min(0.1);
                self.last_dim_tick = now;
                // Exponential interpolation speed (higher = faster fade)
                let fade_speed = 8.0;

                let mut dim_vertices: Vec<RectVertex> = Vec::new();
                let mut any_transitioning = false;
                for info in &frame_glyphs.window_infos {
                    let target = if info.selected { 0.0 } else { self.inactive_dim_opacity };
                    let current = self.per_window_dim.get(&info.window_id).copied().unwrap_or(target);
                    // Exponential interpolation toward target
                    let new_opacity = current + (target - current) * (1.0 - (-fade_speed * dt).exp());
                    // Snap to target when close enough
                    let new_opacity = if (new_opacity - target).abs() < 0.001 { target } else { new_opacity };
                    self.per_window_dim.insert(info.window_id, new_opacity);
                    if (new_opacity - target).abs() > 0.0005 {
                        any_transitioning = true;
                    }
                    if new_opacity > 0.001 {
                        let dim_color = Color::new(0.0, 0.0, 0.0, new_opacity);
                        let b = &info.bounds;
                        self.add_rect(&mut dim_vertices, b.x, b.y, b.width, b.height, &dim_color);
                    }
                }
                // Clean up windows that no longer exist
                let valid_ids: std::collections::HashSet<i64> = frame_glyphs.window_infos.iter()
                    .map(|i| i.window_id).collect();
                self.per_window_dim.retain(|k, _| valid_ids.contains(k));

                if !dim_vertices.is_empty() {
                    let dim_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Inactive Dim Buffer"),
                            contents: bytemuck::cast_slice(&dim_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, dim_buffer.slice(..));
                    render_pass.draw(0..dim_vertices.len() as u32, 0..1);
                }
                // Signal that we need continuous redraws during transition
                if any_transitioning {
                    self.needs_continuous_redraw = true;
                }
            }

            // === Inactive window color tint ===
            if self.inactive_tint_enabled && frame_glyphs.window_infos.len() > 1 {
                let (tr, tg, tb) = self.inactive_tint_color;
                let opacity = self.inactive_tint_opacity.clamp(0.0, 1.0);
                let mut tint_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.selected { continue; }
                    let b = &info.bounds;
                    let tint_color = Color::new(tr, tg, tb, opacity);
                    self.add_rect(&mut tint_vertices, b.x, b.y, b.width, b.height, &tint_color);
                }

                if !tint_vertices.is_empty() {
                    let tint_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Inactive Tint Buffer"),
                            contents: bytemuck::cast_slice(&tint_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, tint_buffer.slice(..));
                    render_pass.draw(0..tint_vertices.len() as u32, 0..1);
                }
            }

            // === Zen mode: draw margin overlays for centered content ===
            if self.zen_mode_enabled {
                let content_pct = self.zen_mode_content_width_pct.clamp(20.0, 100.0) / 100.0;
                let margin_alpha = self.zen_mode_margin_opacity;
                let dim_color = Color::new(0.0, 0.0, 0.0, margin_alpha);
                let mut zen_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;
                    let content_h = b.height - info.mode_line_height;
                    if content_h < 10.0 { continue; }

                    let content_w = b.width * content_pct;
                    let margin_w = (b.width - content_w) / 2.0;

                    if margin_w > 2.0 {
                        // Left margin overlay
                        self.add_rect(&mut zen_vertices,
                            b.x, b.y, margin_w, content_h, &dim_color);
                        // Right margin overlay
                        self.add_rect(&mut zen_vertices,
                            b.x + b.width - margin_w, b.y, margin_w, content_h, &dim_color);
                    }

                    // Dim the mode-line slightly
                    if info.mode_line_height > 0.0 {
                        let ml_dim = Color::new(0.0, 0.0, 0.0, margin_alpha * 0.5);
                        self.add_rect(&mut zen_vertices,
                            b.x, b.y + content_h, b.width, info.mode_line_height, &ml_dim);
                    }
                }

                if !zen_vertices.is_empty() {
                    let zen_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Zen Mode Buffer"),
                            contents: bytemuck::cast_slice(&zen_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, zen_buffer.slice(..));
                    render_pass.draw(0..zen_vertices.len() as u32, 0..1);
                }
            }

            // === Search highlight pulse (glow on isearch face glyphs) ===
            if self.search_pulse_enabled && self.search_pulse_face_id > 0 {
                let target_face = self.search_pulse_face_id;
                // Find bounding box of all glyphs with the isearch face
                let mut min_x = f32::MAX;
                let mut min_y = f32::MAX;
                let mut max_x = f32::MIN;
                let mut max_y = f32::MIN;
                let mut found = false;
                let mut match_bg: Option<Color> = None;

                for glyph in &frame_glyphs.glyphs {
                    if let FrameGlyph::Char { x, y, width, height, face_id, bg, .. } = glyph {
                        if *face_id == target_face {
                            min_x = min_x.min(*x);
                            min_y = min_y.min(*y);
                            max_x = max_x.max(*x + *width);
                            max_y = max_y.max(*y + *height);
                            found = true;
                            if match_bg.is_none() {
                                match_bg = bg.clone();
                            }
                        }
                    }
                }

                if found {
                    let elapsed = self.search_pulse_start.elapsed().as_secs_f32();
                    let phase = elapsed * 3.0 * std::f32::consts::PI; // 1.5 Hz
                    let pulse = (phase.sin() + 1.0) / 2.0; // 0..1

                    // Use the match background color or default to a warm highlight
                    let (pr, pg, pb) = match match_bg {
                        Some(c) => (c.r, c.g, c.b),
                        None => (1.0, 0.8, 0.3), // warm yellow
                    };

                    let glow_alpha = 0.15 + 0.2 * pulse;
                    let glow_pad = 4.0 + 3.0 * pulse; // expanding glow
                    let glow_color = Color::new(pr, pg, pb, glow_alpha);

                    let mut search_vertices: Vec<RectVertex> = Vec::new();
                    self.add_rect(&mut search_vertices,
                        min_x - glow_pad, min_y - glow_pad,
                        (max_x - min_x) + glow_pad * 2.0,
                        (max_y - min_y) + glow_pad * 2.0,
                        &glow_color);

                    if !search_vertices.is_empty() {
                        let search_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Search Pulse Buffer"),
                                contents: bytemuck::cast_slice(&search_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, search_buffer.slice(..));
                        render_pass.draw(0..search_vertices.len() as u32, 0..1);
                    }

                    self.needs_continuous_redraw = true;
                }
            }

            // === Typing ripple effect ===
            if self.typing_ripple_enabled && !self.active_ripples.is_empty() {
                let now = std::time::Instant::now();
                let duration = self.typing_ripple_duration;
                let max_r = self.typing_ripple_max_radius;

                // Remove expired ripples
                self.active_ripples.retain(|&(_, _, t)| now.duration_since(t).as_secs_f32() < duration);

                if !self.active_ripples.is_empty() {
                    let mut ripple_vertices: Vec<RectVertex> = Vec::new();
                    let segments = 32;

                    for &(cx, cy, spawn_t) in &self.active_ripples {
                        let elapsed = now.duration_since(spawn_t).as_secs_f32();
                        let t = (elapsed / duration).min(1.0);

                        // Eased expansion
                        let ease_t = 1.0 - (1.0 - t) * (1.0 - t); // ease-out quadratic
                        let radius = max_r * ease_t;
                        // Fade out
                        let alpha = 0.4 * (1.0 - t);
                        let ring_thickness = 1.5;

                        let color = Color::new(0.5, 0.7, 1.0, alpha);

                        // Draw ring as small rectangles at each angle
                        for i in 0..segments {
                            let angle = (i as f32 / segments as f32) * 2.0 * std::f32::consts::PI;
                            let px = cx + radius * angle.cos();
                            let py = cy + radius * angle.sin();
                            self.add_rect(&mut ripple_vertices,
                                px - ring_thickness / 2.0,
                                py - ring_thickness / 2.0,
                                ring_thickness,
                                ring_thickness,
                                &color);
                        }
                    }

                    if !ripple_vertices.is_empty() {
                        let ripple_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Ripple Buffer"),
                                contents: bytemuck::cast_slice(&ripple_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, ripple_buffer.slice(..));
                        render_pass.draw(0..ripple_vertices.len() as u32, 0..1);
                    }

                    self.needs_continuous_redraw = true;
                }
            }

            // === Minimap: code overview column on right side of each window ===
            if self.minimap_enabled {
                let minimap_w = self.minimap_width;
                let char_w = frame_glyphs.char_width.max(1.0);
                let char_h = frame_glyphs.char_height.max(1.0);
                // Scale factor: each source char maps to this many pixels in minimap
                let scale_x = 2.0_f32;
                let scale_y = 1.5_f32;

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }

                    let b = &info.bounds;
                    // Content area (excluding mode-line)
                    let content_h = b.height - info.mode_line_height;
                    if content_h < 10.0 { continue; }

                    // Minimap positioned at right edge of window content
                    let map_x = b.x + b.width - minimap_w;
                    let map_y = b.y;
                    let map_h = content_h;

                    // Semi-transparent background
                    let bg_color = Color::new(0.0, 0.0, 0.0, 0.15);
                    let mut minimap_vertices: Vec<RectVertex> = Vec::new();
                    self.add_rect(&mut minimap_vertices, map_x, map_y, minimap_w, map_h, &bg_color);

                    // Collect glyphs belonging to this window's content area
                    // and render each as a tiny colored rectangle
                    for glyph in &frame_glyphs.glyphs {
                        if let FrameGlyph::Char { x, y, width, height, fg, char: ch, is_overlay, .. } = glyph {
                            if *is_overlay { continue; }
                            if *ch == ' ' || *ch == '\t' || *ch == '\n' { continue; }
                            // Check glyph is in this window's content area
                            if *x < b.x || *x >= b.x + b.width - minimap_w { continue; }
                            if *y < b.y || *y >= b.y + content_h { continue; }

                            // Map glyph position to minimap coordinates
                            let rel_x = (*x - b.x) / char_w;
                            let rel_y = (*y - b.y) / char_h;
                            let mini_x = map_x + 2.0 + rel_x * scale_x;
                            let mini_y = map_y + rel_y * scale_y;

                            // Skip if outside minimap bounds
                            if mini_x >= map_x + minimap_w - 1.0 { continue; }
                            if mini_y >= map_y + map_h - 1.0 { continue; }

                            // Draw tiny colored block for each character
                            let dot_w = ((*width / char_w) * scale_x).max(1.0).min(scale_x * 2.0);
                            let dot_h = scale_y;
                            let dot_color = Color::new(fg.r, fg.g, fg.b, 0.7);
                            self.add_rect(&mut minimap_vertices, mini_x, mini_y, dot_w, dot_h, &dot_color);
                        }
                    }

                    // Viewport indicator: show where the visible portion is relative to full buffer
                    if info.buffer_size > 0 {
                        let start_frac = info.window_start as f32 / info.buffer_size as f32;
                        let end_frac = (info.window_end as f32 / info.buffer_size as f32).min(1.0);
                        let vp_y = map_y + start_frac * map_h;
                        let vp_h = ((end_frac - start_frac) * map_h).max(4.0);
                        let vp_color = Color::new(1.0, 1.0, 1.0, 0.1);
                        self.add_rect(&mut minimap_vertices, map_x, vp_y, minimap_w, vp_h, &vp_color);
                        // Left edge highlight for viewport indicator
                        let edge_color = Color::new(0.5, 0.7, 1.0, 0.4);
                        self.add_rect(&mut minimap_vertices, map_x, vp_y, 2.0, vp_h, &edge_color);
                    }

                    if !minimap_vertices.is_empty() {
                        let minimap_buffer = self.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Minimap Buffer"),
                                contents: bytemuck::cast_slice(&minimap_vertices),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );
                        render_pass.set_pipeline(&self.rect_pipeline);
                        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        render_pass.set_vertex_buffer(0, minimap_buffer.slice(..));
                        render_pass.draw(0..minimap_vertices.len() as u32, 0..1);
                    }
                }
            }

            // === Header/mode-line shadow depth effect ===
            if self.header_shadow_enabled {
                let shadow_size = self.header_shadow_size.max(1.0);
                let intensity = self.header_shadow_intensity.clamp(0.0, 1.0);
                let steps = 8;
                let mut shadow_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;

                    // Shadow below header-line (if present): at the top of content area
                    // Header-line is at the very top of the window, same height as mode-line typically
                    // We detect header-line by checking for overlay glyphs at the window top
                    // For simplicity, we'll check if there are overlay glyphs near the window top
                    let mut header_bottom: Option<f32> = None;
                    for g in &frame_glyphs.glyphs {
                        if let FrameGlyph::Char { x, y, height, is_overlay: true, .. }
                            | FrameGlyph::Stretch { x, y, height, is_overlay: true, .. } = g
                        {
                            let gx = match g {
                                FrameGlyph::Char { x, .. } => *x,
                                FrameGlyph::Stretch { x, .. } => *x,
                                _ => continue,
                            };
                            let gy = match g {
                                FrameGlyph::Char { y, .. } => *y,
                                FrameGlyph::Stretch { y, .. } => *y,
                                _ => continue,
                            };
                            let gh = match g {
                                FrameGlyph::Char { height, .. } => *height,
                                FrameGlyph::Stretch { height, .. } => *height,
                                _ => continue,
                            };
                            // Check if this overlay glyph is at the top of this window
                            if gx >= b.x && gx < b.x + b.width
                                && (gy - b.y).abs() < 2.0
                            {
                                let bottom = gy + gh;
                                header_bottom = Some(header_bottom.map_or(bottom, |prev: f32| prev.max(bottom)));
                            }
                        }
                    }

                    // Draw downward shadow below header-line
                    if let Some(hb) = header_bottom {
                        for i in 0..steps {
                            let t = i as f32 / steps as f32;
                            let alpha = intensity * (1.0 - t) * (1.0 - t);
                            let strip_h = shadow_size / steps as f32;
                            let sy = hb + i as f32 * strip_h;
                            let color = Color { r: 0.0, g: 0.0, b: 0.0, a: alpha };
                            self.add_rect(&mut shadow_vertices, b.x, sy, b.width, strip_h, &color);
                        }
                    }

                    // Draw upward shadow above mode-line
                    if info.mode_line_height > 0.0 {
                        let ml_top = b.y + b.height - info.mode_line_height;
                        for i in 0..steps {
                            let t = i as f32 / steps as f32;
                            let alpha = intensity * (1.0 - t) * (1.0 - t);
                            let strip_h = shadow_size / steps as f32;
                            let sy = ml_top - (i as f32 + 1.0) * strip_h;
                            let color = Color { r: 0.0, g: 0.0, b: 0.0, a: alpha };
                            self.add_rect(&mut shadow_vertices, b.x, sy, b.width, strip_h, &color);
                        }
                    }
                }

                if !shadow_vertices.is_empty() {
                    let shadow_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Header Shadow Buffer"),
                        contents: bytemuck::cast_slice(&shadow_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, shadow_buffer.slice(..));
                    render_pass.draw(0..shadow_vertices.len() as u32, 0..1);
                }
            }

            // === Active window border glow ===
            if self.window_glow_enabled {
                let glow_radius = self.window_glow_radius.max(1.0);
                let intensity = self.window_glow_intensity.clamp(0.0, 1.0);
                let (cr, cg, cb) = self.window_glow_color;
                let steps = 10;
                let mut glow_vertices: Vec<RectVertex> = Vec::new();

                // Find the selected window
                for info in &frame_glyphs.window_infos {
                    if !info.selected || info.is_minibuffer { continue; }
                    let b = &info.bounds;

                    for i in 0..steps {
                        let t = (i + 1) as f32 / steps as f32;
                        // Quadratic falloff: brightest at edge, fading outward
                        let alpha = intensity * (1.0 - t) * (1.0 - t);
                        let offset = t * glow_radius;
                        let strip_w = glow_radius / steps as f32;
                        let color = Color::new(cr, cg, cb, alpha);

                        // Top edge (outside window, above)
                        self.add_rect(&mut glow_vertices,
                            b.x - offset, b.y - offset,
                            b.width + offset * 2.0, strip_w, &color);
                        // Bottom edge (outside window, below)
                        self.add_rect(&mut glow_vertices,
                            b.x - offset, b.y + b.height + offset - strip_w,
                            b.width + offset * 2.0, strip_w, &color);
                        // Left edge (outside window)
                        self.add_rect(&mut glow_vertices,
                            b.x - offset, b.y - offset,
                            strip_w, b.height + offset * 2.0, &color);
                        // Right edge (outside window)
                        self.add_rect(&mut glow_vertices,
                            b.x + b.width + offset - strip_w, b.y - offset,
                            strip_w, b.height + offset * 2.0, &color);
                    }
                }

                if !glow_vertices.is_empty() {
                    let glow_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Window Glow Buffer"),
                        contents: bytemuck::cast_slice(&glow_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, glow_buffer.slice(..));
                    render_pass.draw(0..glow_vertices.len() as u32, 0..1);
                }
            }

            // === Scroll progress indicator bar ===
            if self.scroll_progress_enabled {
                let bar_h = self.scroll_progress_height.max(1.0);
                let (cr, cg, cb) = self.scroll_progress_color;
                let opacity = self.scroll_progress_opacity.clamp(0.0, 1.0);
                let mut progress_vertices: Vec<RectVertex> = Vec::new();

                for info in &frame_glyphs.window_infos {
                    if info.is_minibuffer { continue; }
                    let b = &info.bounds;
                    let buf_size = info.buffer_size.max(1) as f32;

                    // Compute scroll fraction (0.0 = top, 1.0 = bottom)
                    let frac = (info.window_start as f32 / buf_size).clamp(0.0, 1.0);

                    // Compute visible fraction (how much of buffer is visible)
                    let visible = ((info.window_end - info.window_start).max(1) as f32 / buf_size)
                        .clamp(0.0, 1.0);

                    // Track background (subtle)
                    let track_color = Color::new(cr, cg, cb, opacity * 0.15);
                    self.add_rect(&mut progress_vertices, b.x, b.y, b.width, bar_h, &track_color);

                    // Progress thumb
                    let thumb_w = (visible * b.width).max(4.0);
                    let thumb_x = b.x + frac * (b.width - thumb_w);
                    let thumb_color = Color::new(cr, cg, cb, opacity);
                    self.add_rect(&mut progress_vertices, thumb_x, b.y, thumb_w, bar_h, &thumb_color);
                }

                if !progress_vertices.is_empty() {
                    let progress_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Scroll Progress Buffer"),
                        contents: bytemuck::cast_slice(&progress_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, progress_buffer.slice(..));
                    render_pass.draw(0..progress_vertices.len() as u32, 0..1);
                }
            }

            // === Vignette effect: darken edges of the frame ===
            if self.vignette_enabled {
                let frame_w = frame_glyphs.width;
                let frame_h = frame_glyphs.height;
                let intensity = self.vignette_intensity.clamp(0.0, 1.0);
                let radius_pct = self.vignette_radius.clamp(10.0, 100.0) / 100.0;

                // Number of gradient steps from edge inward
                let steps = 16;
                let mut vignette_vertices: Vec<RectVertex> = Vec::new();

                for i in 0..steps {
                    let t = i as f32 / steps as f32;
                    // Quadratic falloff for smooth gradient
                    let alpha = intensity * t * t;
                    let inset = (1.0 - t) * radius_pct * frame_w.min(frame_h) * 0.5;
                    let strip_w = inset / steps as f32;

                    if strip_w < 0.5 { continue; }

                    let color = Color::new(0.0, 0.0, 0.0, alpha);

                    // Top edge strip
                    let y = inset - strip_w;
                    if y >= 0.0 {
                        self.add_rect(&mut vignette_vertices, 0.0, y, frame_w, strip_w, &color);
                    }
                    // Bottom edge strip
                    let by = frame_h - inset;
                    if by < frame_h {
                        self.add_rect(&mut vignette_vertices, 0.0, by, frame_w, strip_w, &color);
                    }
                    // Left edge strip
                    let lx = inset - strip_w;
                    if lx >= 0.0 {
                        self.add_rect(&mut vignette_vertices, lx, 0.0, strip_w, frame_h, &color);
                    }
                    // Right edge strip
                    let rx = frame_w - inset;
                    if rx < frame_w {
                        self.add_rect(&mut vignette_vertices, rx, 0.0, strip_w, frame_h, &color);
                    }
                }

                if !vignette_vertices.is_empty() {
                    let vignette_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Vignette Buffer"),
                            contents: bytemuck::cast_slice(&vignette_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, vignette_buffer.slice(..));
                    render_pass.draw(0..vignette_vertices.len() as u32, 0..1);
                }
            }

            // === Window switch highlight fade ===
            if !self.active_window_fades.is_empty() {
                let mut fade_vertices: Vec<RectVertex> = Vec::new();
                let now = std::time::Instant::now();

                for fade in &self.active_window_fades {
                    let elapsed = now.duration_since(fade.started);
                    let t = (elapsed.as_secs_f32() / fade.duration.as_secs_f32()).min(1.0);
                    if t >= 1.0 { continue; }
                    // Ease-out: bright flash that fades smoothly
                    let eased = t * (2.0 - t);
                    let alpha = fade.intensity * (1.0 - eased);

                    let color = Color::new(1.0, 1.0, 1.0, alpha);
                    self.add_rect(
                        &mut fade_vertices,
                        fade.bounds.x, fade.bounds.y,
                        fade.bounds.width, fade.bounds.height,
                        &color,
                    );
                }

                // Clean up completed fades
                self.active_window_fades.retain(|f| {
                    f.started.elapsed().as_secs_f32() < f.duration.as_secs_f32()
                });

                if !self.active_window_fades.is_empty() {
                    self.needs_continuous_redraw = true;
                }

                if !fade_vertices.is_empty() {
                    let fade_buffer = self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("Window Switch Fade Buffer"),
                            contents: bytemuck::cast_slice(&fade_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    );
                    render_pass.set_pipeline(&self.rect_pipeline);
                    render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, fade_buffer.slice(..));
                    render_pass.draw(0..fade_vertices.len() as u32, 0..1);
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

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

    /// Render a crossfade transition within a scissor region
    /// Uses the image_pipeline to blend old and new textures
    pub fn render_crossfade(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        blend_t: f32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        // We render two passes: old texture with alpha (1-t), new texture with alpha t
        // Using scissor rect to constrain to the window bounds

        // Scissor rects operate in physical framebuffer pixels; bounds from Emacs are logical
        let sf = self.scale_factor;
        let sx = (bounds.x.max(0.0) * sf) as u32;
        let sy = (bounds.y.max(0.0) * sf) as u32;
        let sw = ((bounds.width * sf) as u32).min(surface_width.saturating_sub(sx));
        let sh = ((bounds.height * sf) as u32).min(surface_height.saturating_sub(sy));

        if sw == 0 || sh == 0 {
            return;
        }

        // Use logical dimensions for vertex positions since screen_size uniform is logical
        let w = surface_width as f32 / sf;
        let h = surface_height as f32 / sf;

        // Fullscreen quad with UV mapping
        let vertices = [
            GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [w, 0.0], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
            GlyphVertex { position: [0.0, h], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
        ];

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Crossfade Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Crossfade Encoder"),
        });

        {
            // Pass 1: Draw old texture with alpha (1 - blend_t)
            let old_alpha = 1.0 - blend_t;
            let old_vertices = [
                GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, old_alpha] },
                GlyphVertex { position: [w, 0.0], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, old_alpha] },
                GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, old_alpha] },
                GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, old_alpha] },
                GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, old_alpha] },
                GlyphVertex { position: [0.0, h], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, old_alpha] },
            ];
            let old_vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Crossfade Old VB"),
                contents: bytemuck::cast_slice(&old_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            // New texture with alpha blend_t
            let new_vertices = [
                GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, blend_t] },
                GlyphVertex { position: [w, 0.0], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, blend_t] },
                GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, blend_t] },
                GlyphVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, blend_t] },
                GlyphVertex { position: [w, h], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, blend_t] },
                GlyphVertex { position: [0.0, h], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, blend_t] },
            ];
            let new_vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Crossfade New VB"),
                contents: bytemuck::cast_slice(&new_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Crossfade Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_scissor_rect(sx, sy, sw, sh);
            render_pass.set_pipeline(&self.image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            // Draw old with fading alpha
            render_pass.set_bind_group(1, old_bind_group, &[]);
            render_pass.set_vertex_buffer(0, old_vb.slice(..));
            render_pass.draw(0..6, 0..1);

            // Draw new with increasing alpha
            render_pass.set_bind_group(1, new_bind_group, &[]);
            render_pass.set_vertex_buffer(0, new_vb.slice(..));
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a scroll slide transition within a scissor region
    ///
    /// Uses content-region UV mapping so only the content area of each offscreen
    /// texture is sampled — the mode-line is never included in the sliding quads.
    pub fn render_scroll_slide(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        // Ease-out quadratic
        let eased_t = 1.0 - (1.0 - t).powi(2);
        let offset = bounds.height * eased_t;

        // Scissor rects operate in physical framebuffer pixels; bounds from Emacs are logical
        let sf = self.scale_factor;
        let sx = (bounds.x.max(0.0) * sf) as u32;
        let sy = (bounds.y.max(0.0) * sf) as u32;
        let sw = ((bounds.width * sf) as u32).min(surface_width.saturating_sub(sx));
        let sh = ((bounds.height * sf) as u32).min(surface_height.saturating_sub(sy));

        if sw == 0 || sh == 0 {
            return;
        }

        // Use logical dimensions for vertex positions since screen_size uniform is logical
        let w = surface_width as f32 / sf;
        let h = surface_height as f32 / sf;
        let dir = direction as f32;

        // UV coordinates for the content region within the full-frame texture.
        // bounds is already content-only (mode-line excluded by caller).
        let uv_left = bounds.x / w;
        let uv_top = bounds.y / h;
        let uv_right = (bounds.x + bounds.width) / w;
        let uv_bottom = (bounds.y + bounds.height) / h;

        // Old texture slides out by offset in direction
        let old_y_offset = -dir * offset;
        // New texture slides in from opposite side
        let new_y_offset = dir * (bounds.height - offset);

        // Build a content-region quad: position covers the content bounds shifted
        // by y_off, UV maps to exactly the content region in the full-frame texture.
        let make_quad = |y_off: f32| -> [GlyphVertex; 6] {
            let x0 = bounds.x;
            let x1 = bounds.x + bounds.width;
            let y0 = bounds.y + y_off;
            let y1 = bounds.y + bounds.height + y_off;
            [
                GlyphVertex { position: [x0, y0], tex_coords: [uv_left, uv_top], color: [1.0, 1.0, 1.0, 1.0] },
                GlyphVertex { position: [x1, y0], tex_coords: [uv_right, uv_top], color: [1.0, 1.0, 1.0, 1.0] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_right, uv_bottom], color: [1.0, 1.0, 1.0, 1.0] },
                GlyphVertex { position: [x0, y0], tex_coords: [uv_left, uv_top], color: [1.0, 1.0, 1.0, 1.0] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_right, uv_bottom], color: [1.0, 1.0, 1.0, 1.0] },
                GlyphVertex { position: [x0, y1], tex_coords: [uv_left, uv_bottom], color: [1.0, 1.0, 1.0, 1.0] },
            ]
        };

        let old_vertices = make_quad(old_y_offset);
        let new_vertices = make_quad(new_y_offset);

        let old_vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scroll Old VB"),
            contents: bytemuck::cast_slice(&old_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let new_vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scroll New VB"),
            contents: bytemuck::cast_slice(&new_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Scroll Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scroll Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_scissor_rect(sx, sy, sw, sh);
            render_pass.set_pipeline(&self.image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            // Draw old texture sliding out
            render_pass.set_bind_group(1, old_bind_group, &[]);
            render_pass.set_vertex_buffer(0, old_vb.slice(..));
            render_pass.draw(0..6, 0..1);

            // Draw new texture sliding in
            render_pass.set_bind_group(1, new_bind_group, &[]);
            render_pass.set_vertex_buffer(0, new_vb.slice(..));
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Dispatch to the appropriate scroll effect renderer.
    ///
    /// This is the main entry point called by `render_transitions()` for each
    /// active scroll transition. It applies the easing function to `raw_t`,
    /// then delegates to the specific effect renderer.
    pub fn render_scroll_effect(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        raw_t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        effect: crate::core::scroll_animation::ScrollEffect,
        easing: crate::core::scroll_animation::ScrollEasing,
        surface_width: u32,
        surface_height: u32,
    ) {
        use crate::core::scroll_animation::ScrollEffect;

        let eased_t = easing.apply(raw_t);

        match effect {
            ScrollEffect::Slide => {
                // Use existing slide renderer (it has its own easing, pass raw_t)
                self.render_scroll_slide(
                    surface_view, old_bind_group, new_bind_group,
                    raw_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Crossfade => {
                self.render_scroll_crossfade(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::ScaleZoom => {
                self.render_scroll_scale_zoom(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::FadeEdges => {
                self.render_scroll_fade_edges(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Cascade => {
                self.render_scroll_cascade(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, elapsed_secs, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Parallax => {
                self.render_scroll_parallax(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Tilt => {
                self.render_scroll_tilt(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::PageCurl => {
                self.render_scroll_page_curl(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::CardFlip => {
                self.render_scroll_card_flip(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::CylinderRoll => {
                self.render_scroll_cylinder_roll(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Wobbly => {
                self.render_scroll_wobbly(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, elapsed_secs, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Wave => {
                self.render_scroll_wave(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, elapsed_secs, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::PerLineSpring => {
                self.render_scroll_per_line_spring(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, elapsed_secs, direction, bounds, surface_width, surface_height,
                );
            }

            ScrollEffect::Liquid => {
                self.render_scroll_liquid(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, elapsed_secs, direction, bounds, surface_width, surface_height,
                );
            }

            // Post-processing effects: render slide first, then apply post-process
            ScrollEffect::MotionBlur
            | ScrollEffect::ChromaticAberration
            | ScrollEffect::GhostTrails
            | ScrollEffect::ColorTemperature
            | ScrollEffect::CRTScanlines
            | ScrollEffect::DepthOfField => {
                self.render_scroll_with_post_process(
                    surface_view, old_bind_group, new_bind_group,
                    raw_t, eased_t, elapsed_secs, direction, bounds,
                    effect, surface_width, surface_height,
                );
            }

            ScrollEffect::TypewriterReveal => {
                self.render_scroll_typewriter(
                    surface_view, old_bind_group, new_bind_group,
                    eased_t, elapsed_secs, direction, bounds, surface_width, surface_height,
                );
            }
        }
    }

    // ── Scroll Effect Implementations ─────────────────────────────────────

    /// Helper: compute scissor rect and content UV from bounds.
    fn scroll_scissor_and_uv(
        &self,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) -> Option<(u32, u32, u32, u32, f32, f32, f32, f32, f32, f32)> {
        let sf = self.scale_factor;
        let sx = (bounds.x.max(0.0) * sf) as u32;
        let sy = (bounds.y.max(0.0) * sf) as u32;
        let sw = ((bounds.width * sf) as u32).min(surface_width.saturating_sub(sx));
        let sh = ((bounds.height * sf) as u32).min(surface_height.saturating_sub(sy));
        if sw == 0 || sh == 0 {
            return None;
        }
        let w = surface_width as f32 / sf;
        let h = surface_height as f32 / sf;
        let uv_left = bounds.x / w;
        let uv_top = bounds.y / h;
        let uv_right = (bounds.x + bounds.width) / w;
        let uv_bottom = (bounds.y + bounds.height) / h;
        Some((sx, sy, sw, sh, w, h, uv_left, uv_top, uv_right, uv_bottom))
    }

    /// Helper: create a vertex buffer from GlyphVertex slice.
    fn create_scroll_vb(&self, vertices: &[GlyphVertex]) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scroll VB"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        })
    }

    /// Helper: submit a two-quad scroll render pass (old + new textures).
    fn submit_scroll_two_quad_pass(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        old_vertices: &[GlyphVertex],
        new_vertices: &[GlyphVertex],
        sx: u32, sy: u32, sw: u32, sh: u32,
    ) {
        use wgpu::util::DeviceExt;
        let old_vb = self.create_scroll_vb(old_vertices);
        let new_vb = self.create_scroll_vb(new_vertices);

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Scroll Effect Encoder"),
        });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scroll Effect Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_scissor_rect(sx, sy, sw, sh);
            rp.set_pipeline(&self.image_pipeline);
            rp.set_bind_group(0, &self.uniform_bind_group, &[]);

            rp.set_bind_group(1, old_bind_group, &[]);
            rp.set_vertex_buffer(0, old_vb.slice(..));
            rp.draw(0..old_vertices.len() as u32, 0..1);

            rp.set_bind_group(1, new_bind_group, &[]);
            rp.set_vertex_buffer(0, new_vb.slice(..));
            rp.draw(0..new_vertices.len() as u32, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Crossfade scroll: alpha blend old → new within content bounds.
    fn render_scroll_crossfade(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };
        let x0 = bounds.x;
        let y0 = bounds.y;
        let x1 = bounds.x + bounds.width;
        let y1 = bounds.y + bounds.height;
        let old_a = 1.0 - t;

        let old_verts = [
            GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, old_a] },
            GlyphVertex { position: [x1, y0], tex_coords: [uv_r, uv_t], color: [1.0, 1.0, 1.0, old_a] },
            GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, old_a] },
            GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, old_a] },
            GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, old_a] },
            GlyphVertex { position: [x0, y1], tex_coords: [uv_l, uv_b], color: [1.0, 1.0, 1.0, old_a] },
        ];
        let new_verts = [
            GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, t] },
            GlyphVertex { position: [x1, y0], tex_coords: [uv_r, uv_t], color: [1.0, 1.0, 1.0, t] },
            GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, t] },
            GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, t] },
            GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, t] },
            GlyphVertex { position: [x0, y1], tex_coords: [uv_l, uv_b], color: [1.0, 1.0, 1.0, t] },
        ];
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// ScaleZoom: old shrinks to 95% and fades; new zooms from 95% to 100%.
    fn render_scroll_scale_zoom(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };
        let cx = bounds.x + bounds.width / 2.0;
        let cy = bounds.y + bounds.height / 2.0;

        // Old: scale from 1.0 → 0.92, fade out
        let old_scale = 1.0 - t * 0.08;
        let old_a = 1.0 - t;
        let old_hw = bounds.width / 2.0 * old_scale;
        let old_hh = bounds.height / 2.0 * old_scale;

        // New: scale from 0.92 → 1.0, fade in
        let new_scale = 0.92 + t * 0.08;
        let new_hw = bounds.width / 2.0 * new_scale;
        let new_hh = bounds.height / 2.0 * new_scale;

        let make_quad = |hw: f32, hh: f32, alpha: f32| -> [GlyphVertex; 6] {
            let x0 = cx - hw;
            let y0 = cy - hh;
            let x1 = cx + hw;
            let y1 = cy + hh;
            [
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y0], tex_coords: [uv_r, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x0, y1], tex_coords: [uv_l, uv_b], color: [1.0, 1.0, 1.0, alpha] },
            ]
        };

        let old_verts = make_quad(old_hw, old_hh, old_a);
        let new_verts = make_quad(new_hw, new_hh, t);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// FadeEdges: slide with soft fade at viewport top/bottom edges.
    fn render_scroll_fade_edges(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let offset = bounds.height * t;
        let num_strips = 16;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let fade_zone = 0.15; // fade over 15% of height at each edge

        let make_strips = |y_off: f32, is_old: bool| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let rel_y0 = bounds.y + i as f32 * strip_h + y_off;
                let rel_y1 = bounds.y + (i + 1) as f32 * strip_h + y_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                // Alpha based on distance from edge
                let center_y = (rel_y0 + rel_y1) / 2.0;
                let in_bounds_t = ((center_y - bounds.y) / bounds.height).clamp(0.0, 1.0);
                let edge_alpha = if in_bounds_t < fade_zone {
                    in_bounds_t / fade_zone
                } else if in_bounds_t > 1.0 - fade_zone {
                    (1.0 - in_bounds_t) / fade_zone
                } else {
                    1.0
                };
                let base_alpha = if is_old { 1.0 - t } else { t };
                let alpha = (base_alpha * edge_alpha).clamp(0.0, 1.0);

                let x0 = bounds.x;
                let x1 = bounds.x + bounds.width;
                let c = [1.0, 1.0, 1.0, alpha];
                verts.push(GlyphVertex { position: [x0, rel_y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, rel_y0], tex_coords: [uv_r, u0], color: c });
                verts.push(GlyphVertex { position: [x1, rel_y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, rel_y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, rel_y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, rel_y1], tex_coords: [uv_l, u1], color: c });
            }
            verts
        };

        let old_y_off = -dir * offset;
        let new_y_off = dir * (bounds.height - offset);
        let old_verts = make_strips(old_y_off, true);
        let new_verts = make_strips(new_y_off, false);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Cascade: lines drop in with staggered delay (waterfall).
    fn render_scroll_cascade(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let dir = direction as f32;
        let stagger = 0.06; // 60ms stagger per line

        let make_cascade_strips = |bind: &wgpu::BindGroup, is_new: bool| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let line_delay = i as f32 * stagger;
                let line_t = ((t - line_delay / 1.0).max(0.0) / (1.0 - line_delay).max(0.01)).min(1.0);
                let eased = 1.0 - (1.0 - line_t).powi(2);

                let base_y = bounds.y + i as f32 * strip_h;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                let (y_off, alpha) = if is_new {
                    (dir * (bounds.height * (1.0 - eased)), eased)
                } else {
                    (-dir * (bounds.height * eased), 1.0 - eased)
                };

                let y0 = base_y + y_off;
                let y1 = base_y + strip_h + y_off;
                let x0 = bounds.x;
                let x1 = bounds.x + bounds.width;
                let c = [1.0, 1.0, 1.0, alpha];

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: c });
            }
            verts
        };

        let old_verts = make_cascade_strips(old_bind_group, false);
        let new_verts = make_cascade_strips(new_bind_group, true);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Parallax: layers scroll at different speeds for depth illusion.
    fn render_scroll_parallax(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        // Foreground scrolls at normal speed, "background" slower
        // We simulate by having old content move slower (0.7x) and new content normal
        let slow_t = t * 0.7;
        let slow_offset = bounds.height * slow_t;
        let fast_offset = bounds.height * t;

        let make_quad = |y_off: f32, alpha: f32| -> [GlyphVertex; 6] {
            let x0 = bounds.x;
            let y0 = bounds.y + y_off;
            let x1 = bounds.x + bounds.width;
            let y1 = bounds.y + bounds.height + y_off;
            [
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y0], tex_coords: [uv_r, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x0, y1], tex_coords: [uv_l, uv_b], color: [1.0, 1.0, 1.0, alpha] },
            ]
        };

        let old_verts = make_quad(-dir * slow_offset, 1.0 - t);
        let new_verts = make_quad(dir * (bounds.height - fast_offset), t);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Tilt: subtle perspective tilt during scroll.
    fn render_scroll_tilt(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let offset = bounds.height * t;
        let tilt_strength = (1.0 - t) * dir; // Tilt decays as animation settles
        let max_tilt = bounds.height * 0.03; // 3% of height
        let num_strips = 12;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;

        let make_tilted = |y_base_off: f32| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt0 = i as f32 / num_strips as f32;
                let nt1 = (i + 1) as f32 / num_strips as f32;

                // Tilt: center stays, edges deflect
                let tilt0 = max_tilt * tilt_strength * (nt0 - 0.5) * 2.0;
                let tilt1 = max_tilt * tilt_strength * (nt1 - 0.5) * 2.0;

                // Horizontal squeeze at edges (perspective)
                let squeeze0 = 1.0 - (nt0 - 0.5).abs() * 0.02 * tilt_strength.abs();
                let squeeze1 = 1.0 - (nt1 - 0.5).abs() * 0.02 * tilt_strength.abs();

                let cx = bounds.x + bounds.width / 2.0;
                let hw0 = bounds.width / 2.0 * squeeze0;
                let hw1 = bounds.width / 2.0 * squeeze1;

                let y0 = bounds.y + i as f32 * strip_h + y_base_off + tilt0;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off + tilt1;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                verts.push(GlyphVertex { position: [cx - hw0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [cx + hw0, y0], tex_coords: [uv_r, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [cx + hw1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [cx - hw0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [cx + hw1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [cx - hw1, y1], tex_coords: [uv_l, u1], color: [1.0; 4] });
            }
            verts
        };

        let old_verts = make_tilted(-dir * offset);
        let new_verts = make_tilted(dir * (bounds.height - offset));
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// PageCurl: page curls away revealing new content underneath.
    fn render_scroll_page_curl(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        use crate::core::scroll_animation::page_curl_transform;
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let num_strips = 24;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;

        // New content: flat, full opacity (drawn first, underneath)
        let new_verts: Vec<GlyphVertex> = {
            let x0 = bounds.x;
            let x1 = bounds.x + bounds.width;
            let y0 = bounds.y;
            let y1 = bounds.y + bounds.height;
            vec![
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0; 4] },
                GlyphVertex { position: [x1, y0], tex_coords: [uv_r, uv_t], color: [1.0; 4] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0; 4] },
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0; 4] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0; 4] },
                GlyphVertex { position: [x0, y1], tex_coords: [uv_l, uv_b], color: [1.0; 4] },
            ]
        };

        // Old content: curling away from bottom (or top for scroll up)
        let old_verts: Vec<GlyphVertex> = {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt = i as f32 / num_strips as f32;
                let nt1 = (i + 1) as f32 / num_strips as f32;

                let curl_pos = if direction > 0 { nt } else { 1.0 - nt };
                let (x_off, y_off, alpha) = page_curl_transform(curl_pos, t, bounds.height);

                let x0 = bounds.x + x_off;
                let x1 = bounds.x + bounds.width + x_off;
                let y0 = bounds.y + i as f32 * strip_h + y_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;
                let c = [1.0, 1.0, 1.0, alpha];

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: c });
            }
            verts
        };

        // Draw new first (underneath), then old (curling on top)
        self.submit_scroll_two_quad_pass(
            surface_view, new_bind_group, old_bind_group,
            &new_verts, &old_verts, sx, sy, sw, sh,
        );
    }

    /// CardFlip: screenful flips like a card around X-axis.
    fn render_scroll_card_flip(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let cx = bounds.x + bounds.width / 2.0;
        let cy = bounds.y + bounds.height / 2.0;

        // Card flip: shrinks height to 0 at midpoint, then expands
        let angle = t * std::f32::consts::PI;
        let scale_y = angle.cos().abs().max(0.02);
        let hh = bounds.height / 2.0 * scale_y;
        let hw = bounds.width / 2.0;

        let (bind_group, alpha) = if t < 0.5 {
            (old_bind_group, 1.0)
        } else {
            (new_bind_group, 1.0)
        };

        let verts = [
            GlyphVertex { position: [cx - hw, cy - hh], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
            GlyphVertex { position: [cx + hw, cy - hh], tex_coords: [uv_r, uv_t], color: [1.0, 1.0, 1.0, alpha] },
            GlyphVertex { position: [cx + hw, cy + hh], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
            GlyphVertex { position: [cx - hw, cy - hh], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
            GlyphVertex { position: [cx + hw, cy + hh], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
            GlyphVertex { position: [cx - hw, cy + hh], tex_coords: [uv_l, uv_b], color: [1.0, 1.0, 1.0, alpha] },
        ];

        // Single texture pass
        let empty: [GlyphVertex; 0] = [];
        self.submit_scroll_two_quad_pass(
            surface_view, bind_group, bind_group,
            &verts, &empty, sx, sy, sw, sh,
        );
    }

    /// CylinderRoll: content wraps around a vertical cylinder.
    fn render_scroll_cylinder_roll(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let num_strips = 16;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let dir = direction as f32;
        let offset = bounds.height * t;
        let pi = std::f32::consts::PI;

        let make_cylinder = |y_base_off: f32, is_old: bool| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            let cx = bounds.x + bounds.width / 2.0;

            for i in 0..num_strips {
                let nt = (i as f32 + 0.5) / num_strips as f32;
                // Angle on the cylinder surface
                let angle = (nt - 0.5) * pi * 0.4 + dir * (1.0 - t) * pi * 0.2;

                let cos_a = angle.cos();
                let squeeze = cos_a.abs().max(0.4);
                let hw = bounds.width / 2.0 * squeeze;

                // Brightness based on angle (facing = bright, edge = dim)
                let brightness = (cos_a * 0.4 + 0.6).clamp(0.3, 1.0);
                let alpha = if is_old { (1.0 - t) * brightness } else { t * brightness };

                let y0 = bounds.y + i as f32 * strip_h + y_base_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;
                let c = [brightness, brightness, brightness, alpha];

                verts.push(GlyphVertex { position: [cx - hw, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [cx + hw, y0], tex_coords: [uv_r, u0], color: c });
                verts.push(GlyphVertex { position: [cx + hw, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [cx - hw, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [cx + hw, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [cx - hw, y1], tex_coords: [uv_l, u1], color: c });
            }
            verts
        };

        let old_verts = make_cylinder(-dir * offset, true);
        let new_verts = make_cylinder(dir * (bounds.height - offset), false);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Wobbly/jelly: content deforms elastically during scroll.
    fn render_scroll_wobbly(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        use crate::core::scroll_animation::wobbly_deform;
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let offset = bounds.height * t;
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let amplitude = bounds.width * 0.03; // 3% of width

        let make_wobbly = |y_base_off: f32| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt = i as f32 / num_strips as f32;
                let (dx, _dy) = wobbly_deform(i, num_strips, nt, t, dir, amplitude);

                let x0 = bounds.x + dx;
                let x1 = bounds.x + bounds.width + dx;
                let y0 = bounds.y + i as f32 * strip_h + y_base_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: [1.0; 4] });
            }
            verts
        };

        let old_verts = make_wobbly(-dir * offset);
        let new_verts = make_wobbly(dir * (bounds.height - offset));
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Wave: horizontal sine-wave displacement during scroll.
    fn render_scroll_wave(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let offset = bounds.height * t;
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let amplitude = bounds.width * 0.025;
        let damping = 1.0 - t;

        let make_wave = |y_base_off: f32| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt = i as f32 / num_strips as f32;
                let phase = nt * std::f32::consts::PI * 4.0 + elapsed_secs * 8.0;
                let dx = amplitude * phase.sin() * damping;

                let x0 = bounds.x + dx;
                let x1 = bounds.x + bounds.width + dx;
                let y0 = bounds.y + i as f32 * strip_h + y_base_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: [1.0; 4] });
            }
            verts
        };

        let old_verts = make_wave(-dir * offset);
        let new_verts = make_wave(dir * (bounds.height - offset));
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// PerLineSpring: each line on own spring with stagger delay.
    fn render_scroll_per_line_spring(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let stagger = 0.015; // 15ms stagger per line

        let make_spring = |is_new: bool| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let line_start = i as f32 * stagger;
                let line_t = ((elapsed_secs - line_start).max(0.0) * 8.0).min(1.0);

                // Spring overshoot: goes past 1.0 then settles
                let omega = 10.0;
                let spring_t = if line_t >= 1.0 {
                    1.0
                } else {
                    let et = (-omega * line_t).exp();
                    1.0 - (1.0 + omega * line_t) * et
                };

                let line_offset = bounds.height * spring_t;
                let y_off = if is_new {
                    dir * (bounds.height - line_offset)
                } else {
                    -dir * line_offset
                };

                let x0 = bounds.x;
                let x1 = bounds.x + bounds.width;
                let y0 = bounds.y + i as f32 * strip_h + y_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: [1.0; 4] });
            }
            verts
        };

        let old_verts = make_spring(false);
        let new_verts = make_spring(true);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Liquid: noise-based UV warping, text ripples like water.
    fn render_scroll_liquid(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        use crate::core::scroll_animation::liquid_deform;
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let offset = bounds.height * t;
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let amplitude = bounds.width * 0.04;

        let make_liquid = |y_base_off: f32| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt = i as f32 / num_strips as f32;
                let (dx, dy) = liquid_deform(i, num_strips, nt, t, elapsed_secs, amplitude);

                let x0 = bounds.x + dx;
                let x1 = bounds.x + bounds.width + dx;
                let y0 = bounds.y + i as f32 * strip_h + y_base_off + dy;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off + dy;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: [1.0; 4] });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: [1.0; 4] });
            }
            verts
        };

        let old_verts = make_liquid(-dir * offset);
        let new_verts = make_liquid(dir * (bounds.height - offset));
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Post-processing scroll effects: render slide, then tint/distort via color manipulation.
    ///
    /// Since we don't have a separate post-process shader pipeline yet, we approximate
    /// post-processing effects by manipulating vertex colors during the slide transition.
    fn render_scroll_with_post_process(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        raw_t: f32,
        eased_t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        effect: crate::core::scroll_animation::ScrollEffect,
        surface_width: u32,
        surface_height: u32,
    ) {
        use crate::core::scroll_animation::ScrollEffect;

        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let offset = bounds.height * eased_t;
        let speed = (1.0 - eased_t); // High at start, low at end
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;

        let make_postprocess = |y_base_off: f32, is_old: bool| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt = i as f32 / num_strips as f32;
                let nt_center = (nt - 0.5).abs() * 2.0; // 0 at center, 1 at edges

                let mut r = 1.0_f32;
                let mut g = 1.0_f32;
                let mut b = 1.0_f32;
                let mut alpha = 1.0_f32;
                let mut dx = 0.0_f32;

                match effect {
                    ScrollEffect::MotionBlur => {
                        // Simulate blur by reducing alpha at edges proportional to speed
                        let blur = speed * 0.4;
                        alpha = 1.0 - nt_center * blur;
                    }
                    ScrollEffect::ChromaticAberration => {
                        // Shift color channels based on position and speed
                        let shift = speed * 0.08;
                        r = 1.0 + shift * (nt - 0.5);
                        b = 1.0 - shift * (nt - 0.5);
                    }
                    ScrollEffect::GhostTrails => {
                        // Reduced alpha creates ghost-like transparency
                        let ghost = speed * 0.3;
                        alpha = 1.0 - ghost * nt_center;
                    }
                    ScrollEffect::ColorTemperature => {
                        // Warm (orange) scrolling down, cool (blue) scrolling up
                        let temp = dir * speed * 0.06;
                        r = (1.0 + temp).clamp(0.9, 1.1);
                        b = (1.0 - temp).clamp(0.9, 1.1);
                    }
                    ScrollEffect::CRTScanlines => {
                        // Scanline brightness modulation
                        let scanline = ((nt * num_strips as f32 * 2.0
                            + elapsed_secs * 20.0).sin() * 0.5 + 0.5);
                        let intensity = 1.0 - speed * 0.15 * scanline;
                        r = intensity;
                        g = intensity;
                        b = intensity;
                    }
                    ScrollEffect::DepthOfField => {
                        // Edges get dimmer (simulating blur)
                        let dof = speed * 0.3;
                        let brightness = 1.0 - nt_center * dof;
                        r = brightness;
                        g = brightness;
                        b = brightness;
                    }
                    _ => {}
                }

                let x0 = bounds.x + dx;
                let x1 = bounds.x + bounds.width + dx;
                let y0 = bounds.y + i as f32 * strip_h + y_base_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;
                let c = [r, g, b, alpha];

                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y0], tex_coords: [uv_r, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, y0], tex_coords: [uv_l, u0], color: c });
                verts.push(GlyphVertex { position: [x1, y1], tex_coords: [uv_r, u1], color: c });
                verts.push(GlyphVertex { position: [x0, y1], tex_coords: [uv_l, u1], color: c });
            }
            verts
        };

        let old_verts = make_postprocess(-dir * offset, true);
        let new_verts = make_postprocess(dir * (bounds.height - offset), false);
        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// TypewriterReveal: new lines appear character-by-character (simulated with strips).
    fn render_scroll_typewriter(
        &self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: i32,
        bounds: &crate::core::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.scroll_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction as f32;
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let stagger = 0.04; // 40ms per line

        // Old content fades out quickly
        let old_verts: Vec<GlyphVertex> = {
            let alpha = (1.0 - t * 2.0).max(0.0);
            let x0 = bounds.x;
            let x1 = bounds.x + bounds.width;
            let y0 = bounds.y;
            let y1 = bounds.y + bounds.height;
            vec![
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y0], tex_coords: [uv_r, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x0, y0], tex_coords: [uv_l, uv_t], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x1, y1], tex_coords: [uv_r, uv_b], color: [1.0, 1.0, 1.0, alpha] },
                GlyphVertex { position: [x0, y1], tex_coords: [uv_l, uv_b], color: [1.0, 1.0, 1.0, alpha] },
            ]
        };

        // New content: each line reveals left-to-right with stagger
        let new_verts: Vec<GlyphVertex> = {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let line_delay = i as f32 * stagger;
                let line_t = ((t - line_delay).max(0.0) / (1.0 - line_delay).max(0.01)).min(1.0);

                // Reveal from left: only show portion of UV
                let reveal = line_t;
                let uv_reveal_right = uv_l + (uv_r - uv_l) * reveal;
                let x_right = bounds.x + bounds.width * reveal;

                let y0 = bounds.y + i as f32 * strip_h;
                let y1 = bounds.y + (i + 1) as f32 * strip_h;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;
                let alpha = line_t;

                verts.push(GlyphVertex { position: [bounds.x, y0], tex_coords: [uv_l, u0], color: [1.0, 1.0, 1.0, alpha] });
                verts.push(GlyphVertex { position: [x_right, y0], tex_coords: [uv_reveal_right, u0], color: [1.0, 1.0, 1.0, alpha] });
                verts.push(GlyphVertex { position: [x_right, y1], tex_coords: [uv_reveal_right, u1], color: [1.0, 1.0, 1.0, alpha] });
                verts.push(GlyphVertex { position: [bounds.x, y0], tex_coords: [uv_l, u0], color: [1.0, 1.0, 1.0, alpha] });
                verts.push(GlyphVertex { position: [x_right, y1], tex_coords: [uv_reveal_right, u1], color: [1.0, 1.0, 1.0, alpha] });
                verts.push(GlyphVertex { position: [bounds.x, y1], tex_coords: [uv_l, u1], color: [1.0, 1.0, 1.0, alpha] });
            }
            verts
        };

        self.submit_scroll_two_quad_pass(
            surface_view, old_bind_group, new_bind_group,
            &old_verts, &new_verts, sx, sy, sw, sh,
        );
    }

    /// Render floating videos from the scene.
    ///
    /// This renders video frames at fixed screen positions (not inline with text).
    #[cfg(feature = "video")]
    pub fn render_floating_videos(
        &self,
        view: &wgpu::TextureView,
        floating_videos: &[crate::core::scene::FloatingVideo],
    ) {
        use wgpu::util::DeviceExt;

        if floating_videos.is_empty() {
            return;
        }

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Floating Video Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Floating Video Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Don't clear - render on top
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            for fv in floating_videos {
                log::debug!("Rendering floating video {} at ({}, {}) size {}x{}",
                           fv.video_id, fv.x, fv.y, fv.width, fv.height);

                if let Some(cached) = self.video_cache.get(fv.video_id) {
                    if let Some(ref bind_group) = cached.bind_group {
                        let vertices = [
                            GlyphVertex { position: [fv.x, fv.y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x + fv.width, fv.y], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x + fv.width, fv.y + fv.height], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x, fv.y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x + fv.width, fv.y + fv.height], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x, fv.y + fv.height], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                        ];

                        let video_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Floating Video Vertex Buffer"),
                            contents: bytemuck::cast_slice(&vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                        render_pass.set_bind_group(1, bind_group, &[]);
                        render_pass.set_vertex_buffer(0, video_buffer.slice(..));
                        render_pass.draw(0..6, 0..1);
                    } else {
                        log::debug!("Video {} has no bind_group yet", fv.video_id);
                    }
                } else {
                    log::debug!("Video {} not found in cache", fv.video_id);
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a WebKit view texture at the given bounds.
    ///
    /// This method renders the WebKit view content (from a wgpu texture)
    /// to the screen at the specified rectangle.
    ///
    /// # Arguments
    /// * `_encoder` - The command encoder to use for rendering
    /// * `_view` - The output texture view to render to
    /// * `_webkit_bind_group` - The bind group containing the WebKit texture
    /// * `_bounds` - The rectangle where the WebKit view should be rendered
    #[cfg(feature = "wpe-webkit")]
    pub fn render_webkit_view(
        &mut self,
        _encoder: &mut wgpu::CommandEncoder,
        _view: &wgpu::TextureView,
        _webkit_bind_group: &wgpu::BindGroup,
        _bounds: crate::core::types::Rect,
    ) {
        // TODO: Implement texture rendering
    }

    /// Render a popup menu overlay on top of all content.
    pub fn render_popup_menu(
        &self,
        view: &wgpu::TextureView,
        menu: &crate::render_thread::PopupMenuState,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Derive colors from face colors if provided, otherwise use defaults.
        let (fg_r, fg_g, fg_b) = menu.face_fg.unwrap_or((0.9, 0.9, 0.9));
        let (bg_r, bg_g, bg_b) = menu.face_bg.unwrap_or((0.15, 0.15, 0.18));

        let bg_color = Color::new(bg_r, bg_g, bg_b, 0.95).srgb_to_linear();
        let border_color = Color::new(
            (bg_r * 0.6 + 0.15).min(1.0),
            (bg_g * 0.6 + 0.15).min(1.0),
            (bg_b * 0.6 + 0.15).min(1.0),
            1.0,
        ).srgb_to_linear();
        let hover_color = Color::new(
            bg_r * 0.5 + fg_r * 0.3,
            bg_g * 0.5 + fg_g * 0.3,
            bg_b * 0.5 + fg_b * 0.3,
            0.9,
        ).srgb_to_linear();
        let text_color = {
            let c = Color::new(fg_r, fg_g, fg_b, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let disabled_color = {
            let c = Color::new(
                fg_r * 0.5 + bg_r * 0.5,
                fg_g * 0.5 + bg_g * 0.5,
                fg_b * 0.5 + bg_b * 0.5,
                1.0,
            ).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let separator_color = Color::new(
            bg_r * 0.7 + fg_r * 0.3,
            bg_g * 0.7 + fg_g * 0.3,
            bg_b * 0.7 + fg_b * 0.3,
            0.8,
        ).srgb_to_linear();
        let title_color = {
            let c = Color::new(
                fg_r * 0.8 + bg_r * 0.2,
                fg_g * 0.8 + bg_g * 0.2,
                fg_b * 0.85 + bg_b * 0.15,
                1.0,
            ).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let shortcut_color = {
            let c = Color::new(
                fg_r * 0.65 + bg_r * 0.35,
                fg_g * 0.65 + bg_g * 0.35,
                fg_b * 0.65 + bg_b * 0.35,
                1.0,
            ).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        let padding = 4.0_f32;
        let font_size = glyph_atlas.default_font_size();
        let char_width = font_size * 0.6;
        let font_size_bits = 0.0_f32.to_bits();

        // Render each panel (root + open submenus)
        let panels = menu.panels();
        for (panel_idx, panel) in panels.iter().enumerate() {
            let (mx, my, mw, mh) = panel.bounds;

            // === Pass 1: Background rectangles ===
            let mut rect_vertices: Vec<RectVertex> = Vec::new();

            // Drop shadow
            let shadow_layers = 4;
            for i in 1..=shadow_layers {
                let offset = i as f32 * 1.5;
                let alpha = 0.12 * (1.0 - (i - 1) as f32 / shadow_layers as f32);
                let shadow = Color::new(0.0, 0.0, 0.0, alpha);
                self.add_rect(&mut rect_vertices, mx + offset, my + offset, mw, mh, &shadow);
            }

            // Background
            self.add_rect(&mut rect_vertices, mx, my, mw, mh, &bg_color);

            // Border
            let bw = 1.0_f32;
            self.add_rect(&mut rect_vertices, mx, my, mw, bw, &border_color);
            self.add_rect(&mut rect_vertices, mx, my + mh - bw, mw, bw, &border_color);
            self.add_rect(&mut rect_vertices, mx, my, bw, mh, &border_color);
            self.add_rect(&mut rect_vertices, mx + mw - bw, my, bw, mh, &border_color);

            // Hover highlight
            if panel.hover_index >= 0 && (panel.hover_index as usize) < panel.item_indices.len() {
                let idx = panel.hover_index as usize;
                let iy = my + panel.item_offsets[idx];
                self.add_rect(&mut rect_vertices, mx + bw, iy, mw - 2.0 * bw, panel.item_height, &hover_color);
            }

            // Separators
            for (i, &item_idx) in panel.item_indices.iter().enumerate() {
                if menu.all_items[item_idx].separator {
                    let iy = my + panel.item_offsets[i] + 3.0;
                    self.add_rect(&mut rect_vertices, mx + 8.0, iy, mw - 16.0, 1.0, &separator_color);
                }
            }

            // Title separator (root panel only)
            if panel_idx == 0 {
                if menu.title.is_some() {
                    let sep_y = my + panel.item_height + 2.0;
                    self.add_rect(&mut rect_vertices, mx + 4.0, sep_y, mw - 8.0, 1.0, &separator_color);
                }
            }

            // Submit rect pass
            if !rect_vertices.is_empty() {
                let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Popup Menu Rect Buffer"),
                    contents: bytemuck::cast_slice(&rect_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Popup Menu Rect Encoder"),
                });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Popup Menu Rect Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    pass.set_pipeline(&self.rect_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_vertex_buffer(0, rect_buffer.slice(..));
                    pass.draw(0..rect_vertices.len() as u32, 0..1);
                }
                self.queue.submit(Some(encoder.finish()));
            }

            // === Pass 2: Text glyphs ===
            let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();

            // Title (root panel only)
            if panel_idx == 0 {
                if let Some(ref title) = menu.title {
                    let tx = mx + padding * 2.0;
                    for (ci, ch) in title.chars().enumerate() {
                        let key = GlyphKey {
                            charcode: ch as u32,
                            face_id: 0,
                            font_size_bits,
                        };
                        glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                        overlay_glyphs.push((key, tx + (ci as f32) * char_width, my + padding, title_color));
                    }
                }
            }

            // Menu items
            for (i, &item_idx) in panel.item_indices.iter().enumerate() {
                let item = &menu.all_items[item_idx];
                if item.separator {
                    continue;
                }
                let iy = my + panel.item_offsets[i];
                let color = if !item.enabled { disabled_color } else { text_color };

                let label_x = mx + padding * 2.0;
                for (ci, ch) in item.label.chars().enumerate() {
                    let key = GlyphKey {
                        charcode: ch as u32,
                        face_id: 0,
                        font_size_bits,
                    };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    overlay_glyphs.push((key, label_x + (ci as f32) * char_width, iy + 2.0, color));
                }

                if !item.shortcut.is_empty() {
                    let shortcut_x = mx + mw - padding * 2.0 - (item.shortcut.len() as f32 * char_width);
                    for (ci, ch) in item.shortcut.chars().enumerate() {
                        let key = GlyphKey {
                            charcode: ch as u32,
                            face_id: 0,
                            font_size_bits,
                        };
                        glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                        overlay_glyphs.push((key, shortcut_x + (ci as f32) * char_width, iy + 2.0, shortcut_color));
                    }
                }

                if item.submenu {
                    let arrow_x = mx + mw - padding * 2.0 - char_width;
                    let key = GlyphKey {
                        charcode: '\u{25B8}' as u32,
                        face_id: 0,
                        font_size_bits,
                    };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    overlay_glyphs.push((key, arrow_x, iy + 2.0, text_color));
                }
            }

            self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
        }
    }

    /// Render a batch of overlay glyphs in a single render pass.
    ///
    /// Each entry is (GlyphKey, x, y, color). Glyphs are sorted by key
    /// so identical characters share a single bind_group switch, and all
    /// rendering happens in one encoder submit instead of one per glyph.
    fn render_overlay_glyphs(
        &self,
        view: &wgpu::TextureView,
        glyphs: &mut Vec<(GlyphKey, f32, f32, [f32; 4])>,
        glyph_atlas: &WgpuGlyphAtlas,
    ) {
        use wgpu::util::DeviceExt;

        if glyphs.is_empty() {
            return;
        }

        // Sort by key for batching consecutive same-texture draws
        glyphs.sort_by(|a, b| {
            a.0.face_id.cmp(&b.0.face_id)
                .then(a.0.font_size_bits.cmp(&b.0.font_size_bits))
                .then(a.0.charcode.cmp(&b.0.charcode))
        });

        // Build vertex buffer for all glyphs at once
        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(glyphs.len() * 6);
        let mut valid: Vec<bool> = Vec::with_capacity(glyphs.len());

        for (key, x, y, color) in glyphs.iter() {
            if let Some(cached) = glyph_atlas.get(key) {
                let gw = cached.width as f32;
                let gh = cached.height as f32;
                let gx = *x + cached.bearing_x;
                let gy = *y - cached.bearing_y + 14.0;

                vertices.extend_from_slice(&[
                    GlyphVertex { position: [gx, gy], tex_coords: [0.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy], tex_coords: [1.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy + gh], tex_coords: [1.0, 1.0], color: *color },
                    GlyphVertex { position: [gx, gy], tex_coords: [0.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy + gh], tex_coords: [1.0, 1.0], color: *color },
                    GlyphVertex { position: [gx, gy + gh], tex_coords: [0.0, 1.0], color: *color },
                ]);
                valid.push(true);
            } else {
                valid.push(false);
            }
        }

        if vertices.is_empty() {
            return;
        }

        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Overlay Glyph Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Overlay Glyph Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay Glyph Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.image_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));

            // Batch draw calls: consecutive same-key glyphs share one bind_group
            let mut vert_idx = 0u32;
            let mut i = 0;
            while i < glyphs.len() {
                if !valid[i] {
                    i += 1;
                    continue;
                }
                let (ref key, _, _, _) = glyphs[i];
                if let Some(cached) = glyph_atlas.get(key) {
                    if cached.is_color {
                        pass.set_pipeline(&self.opaque_image_pipeline);
                    } else {
                        pass.set_pipeline(&self.image_pipeline);
                    }
                    pass.set_bind_group(1, &cached.bind_group, &[]);
                    let batch_start = vert_idx;
                    vert_idx += 6;
                    i += 1;
                    while i < glyphs.len() && valid[i] && glyphs[i].0 == *key {
                        vert_idx += 6;
                        i += 1;
                    }
                    pass.draw(batch_start..vert_idx, 0..1);
                } else {
                    i += 1;
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Render a tooltip overlay on top of the scene.
    pub fn render_tooltip(
        &self,
        view: &wgpu::TextureView,
        tooltip: &crate::render_thread::TooltipState,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let (tx, ty, tw, th) = tooltip.bounds;

        // Convert user-specified colors to linear space (surface is sRGB)
        let bg_color = Color::new(tooltip.bg.0, tooltip.bg.1, tooltip.bg.2, 0.95).srgb_to_linear();
        let border_color = Color::new(
            (tooltip.bg.0 * 0.6 + 0.15).min(1.0),
            (tooltip.bg.1 * 0.6 + 0.15).min(1.0),
            (tooltip.bg.2 * 0.6 + 0.15).min(1.0),
            1.0,
        ).srgb_to_linear();
        let text_color = {
            let c = Color::new(tooltip.fg.0, tooltip.fg.1, tooltip.fg.2, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        // === Pass 1: Background and border rectangles ===
        let mut rect_vertices: Vec<RectVertex> = Vec::new();

        // Drop shadow (layered for soft edge)
        let shadow_layers = 3;
        for i in 1..=shadow_layers {
            let offset = i as f32 * 1.0;
            let alpha = 0.10 * (1.0 - (i - 1) as f32 / shadow_layers as f32);
            let shadow = Color::new(0.0, 0.0, 0.0, alpha);
            self.add_rect(&mut rect_vertices,
                          tx + offset, ty + offset, tw, th, &shadow);
        }

        // Background
        self.add_rect(&mut rect_vertices, tx, ty, tw, th, &bg_color);

        // Border (1px)
        let bw = 1.0_f32;
        self.add_rect(&mut rect_vertices, tx, ty, tw, bw, &border_color); // top
        self.add_rect(&mut rect_vertices, tx, ty + th - bw, tw, bw, &border_color); // bottom
        self.add_rect(&mut rect_vertices, tx, ty, bw, th, &border_color); // left
        self.add_rect(&mut rect_vertices, tx + tw - bw, ty, bw, th, &border_color); // right

        if !rect_vertices.is_empty() {
            let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Tooltip Rect Buffer"),
                contents: bytemuck::cast_slice(&rect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tooltip Rect Encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Tooltip Rect Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // === Pass 2: Collect all text glyphs and render batched ===
        let padding = 6.0_f32;
        let line_height = glyph_atlas.default_line_height();
        let char_width = glyph_atlas.default_font_size() * 0.6;
        let font_size_bits = 0.0_f32.to_bits();
        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();

        for (line_idx, line) in tooltip.lines.iter().enumerate() {
            let ly = ty + padding + line_idx as f32 * line_height;
            for (ci, ch) in line.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: 0,
                    font_size_bits,
                };
                glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                overlay_glyphs.push((key, tx + padding + (ci as f32) * char_width, ly, text_color));
            }
        }

        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    /// Render a custom title bar overlay for borderless/undecorated windows.
    /// Draws a dark bar at the top with the window title and close/maximize/minimize buttons.
    pub fn render_custom_titlebar(
        &self,
        view: &wgpu::TextureView,
        title: &str,
        titlebar_height: f32,
        hover: u32,
        frame_bg: Option<(f32, f32, f32)>,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let tb_h = titlebar_height;
        let btn_w = 46.0_f32;

        // Derive colors from frame background (already in linear space) or fallback
        let bg_color = if let Some((r, g, b)) = frame_bg {
            // Slightly darken the frame bg for the title bar
            Color::new(r * 0.85, g * 0.85, b * 0.85, 0.95)
        } else {
            Color::new(0.12, 0.12, 0.14, 0.95).srgb_to_linear()
        };
        // Determine if theme is light or dark based on luminance
        let luminance = bg_color.r * 0.299 + bg_color.g * 0.587 + bg_color.b * 0.114;
        let is_light = luminance > 0.3;

        let border_color = if is_light {
            Color::new(bg_color.r * 0.8, bg_color.g * 0.8, bg_color.b * 0.8, 1.0)
        } else {
            Color::new(
                (bg_color.r + 0.05).min(1.0),
                (bg_color.g + 0.05).min(1.0),
                (bg_color.b + 0.05).min(1.0),
                1.0,
            )
        };
        let close_hover_color = Color::new(0.9, 0.2, 0.2, 0.9).srgb_to_linear();
        let btn_hover_color = if is_light {
            Color::new(0.0, 0.0, 0.0, 0.1)
        } else {
            Color::new(1.0, 1.0, 1.0, 0.1)
        };
        let text_color = if is_light {
            let c = Color::new(0.15, 0.15, 0.15, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        } else {
            let c = Color::new(0.8, 0.8, 0.82, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let btn_icon_color = if is_light {
            let c = Color::new(0.3, 0.3, 0.3, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        } else {
            let c = Color::new(0.7, 0.7, 0.72, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let close_icon_hover = {
            let c = Color::new(1.0, 1.0, 1.0, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        // === Pass 1: Background and button rectangles ===
        let mut rect_vertices: Vec<RectVertex> = Vec::new();

        // Title bar background
        self.add_rect(&mut rect_vertices, 0.0, 0.0, logical_w, tb_h, &bg_color);

        // Bottom border (1px)
        self.add_rect(&mut rect_vertices, 0.0, tb_h - 1.0, logical_w, 1.0, &border_color);

        // Button positions
        let close_x = logical_w - btn_w;
        let max_x = logical_w - btn_w * 2.0;
        let min_x = logical_w - btn_w * 3.0;

        // Button hover highlights
        // hover: 0=none, 2=close, 3=maximize, 4=minimize
        if hover == 2 {
            self.add_rect(&mut rect_vertices, close_x, 0.0, btn_w, tb_h, &close_hover_color);
        } else if hover == 3 {
            self.add_rect(&mut rect_vertices, max_x, 0.0, btn_w, tb_h, &btn_hover_color);
        } else if hover == 4 {
            self.add_rect(&mut rect_vertices, min_x, 0.0, btn_w, tb_h, &btn_hover_color);
        }

        // Subtle button separator lines
        let sep_color = Color::new(0.2, 0.2, 0.22, 0.5).srgb_to_linear();
        self.add_rect(&mut rect_vertices, close_x, 4.0, 1.0, tb_h - 8.0, &sep_color);
        self.add_rect(&mut rect_vertices, max_x, 4.0, 1.0, tb_h - 8.0, &sep_color);
        self.add_rect(&mut rect_vertices, min_x, 4.0, 1.0, tb_h - 8.0, &sep_color);

        // Render rect pass
        if !rect_vertices.is_empty() {
            let rect_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Titlebar Rect Buffer"),
                        contents: bytemuck::cast_slice(&rect_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

            let mut encoder =
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Titlebar Rect Encoder"),
                    });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Titlebar Rect Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // === Pass 2: Title text and button icons ===
        let font_size = glyph_atlas.default_font_size();
        let char_width = font_size * 0.6;
        let font_size_bits = 0.0_f32.to_bits();
        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();

        // Center title text
        let title_pixel_width = title.chars().count() as f32 * char_width;
        let title_x = (logical_w - title_pixel_width) / 2.0;
        let title_y = (tb_h - font_size) / 2.0;

        for (ci, ch) in title.chars().enumerate() {
            let key = GlyphKey {
                charcode: ch as u32,
                face_id: 0,
                font_size_bits,
            };
            glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
            overlay_glyphs.push((key, title_x + ci as f32 * char_width, title_y, text_color));
        }

        // Button icons: minimize (─), maximize (□), close (×)
        let btn_center_y = (tb_h - font_size) / 2.0;
        let min_color = if hover == 4 { text_color } else { btn_icon_color };
        let max_color = if hover == 3 { text_color } else { btn_icon_color };
        let close_color = if hover == 2 { close_icon_hover } else { btn_icon_color };

        // Minimize: ─ (U+2500)
        let min_icon_x = min_x + (btn_w - char_width) / 2.0;
        let min_key = GlyphKey { charcode: 0x2500, face_id: 0, font_size_bits };
        glyph_atlas.get_or_create(&self.device, &self.queue, &min_key, None);
        overlay_glyphs.push((min_key, min_icon_x, btn_center_y, min_color));

        // Maximize: □ (U+25A1)
        let max_icon_x = max_x + (btn_w - char_width) / 2.0;
        let max_key = GlyphKey { charcode: 0x25A1, face_id: 0, font_size_bits };
        glyph_atlas.get_or_create(&self.device, &self.queue, &max_key, None);
        overlay_glyphs.push((max_key, max_icon_x, btn_center_y, max_color));

        // Close: × (U+00D7)
        let close_icon_x = close_x + (btn_w - char_width) / 2.0;
        let close_key = GlyphKey { charcode: 0x00D7, face_id: 0, font_size_bits };
        glyph_atlas.get_or_create(&self.device, &self.queue, &close_key, None);
        overlay_glyphs.push((close_key, close_icon_x, btn_center_y, close_color));

        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    /// Render thin scroll position indicators on the right edge of each window.
    pub fn render_scroll_indicators(
        &self,
        view: &wgpu::TextureView,
        window_infos: &[crate::core::frame_glyphs::WindowInfo],
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        let indicator_width = 3.0_f32;
        let multi_window = window_infos.len() > 1;

        for info in window_infos {
            // Focus ring for selected window (only when multiple windows visible)
            if multi_window && info.selected {
                let b = &info.bounds;
                let bw = 2.0_f32;
                let accent = Color::new(0.3, 0.5, 0.9, 0.4).srgb_to_linear();
                // Top
                self.add_rect(&mut rect_vertices, b.x, b.y, b.width, bw, &accent);
                // Bottom (above mode-line)
                let bottom_y = b.y + b.height - info.mode_line_height - bw;
                self.add_rect(&mut rect_vertices, b.x, bottom_y, b.width, bw, &accent);
                // Left
                self.add_rect(&mut rect_vertices, b.x, b.y, bw, b.height - info.mode_line_height, &accent);
                // Right
                self.add_rect(&mut rect_vertices, b.x + b.width - bw, b.y, bw, b.height - info.mode_line_height, &accent);
            }

            // Skip windows with no meaningful buffer content for scroll indicator
            if info.buffer_size <= 1 {
                continue;
            }

            let b = &info.bounds;
            // Content area height (exclude mode-line)
            let content_h = b.height - info.mode_line_height;
            if content_h < 20.0 {
                continue;
            }

            // Scroll ratio: what fraction of the buffer is before window_start
            let start_ratio = (info.window_start as f32 - 1.0).max(0.0)
                / (info.buffer_size as f32 - 1.0).max(1.0);

            // Viewport ratio: what fraction of the buffer is visible
            let visible_chars = if info.window_end > 0 {
                (info.window_end - info.window_start).max(1) as f32
            } else {
                // Estimate: content_h worth of text
                content_h * 2.0 // rough chars estimate
            };
            let viewport_ratio = (visible_chars / info.buffer_size as f32).clamp(0.02, 1.0);

            // Indicator bar position and size
            let bar_h = (content_h * viewport_ratio).max(8.0).min(content_h);
            let bar_y = b.y + start_ratio * (content_h - bar_h);

            // Semi-transparent indicator color
            let color = Color::new(0.5, 0.5, 0.5, 0.25).srgb_to_linear();
            let x = b.x + b.width - indicator_width;

            self.add_rect(&mut rect_vertices, x, bar_y, indicator_width, bar_h, &color);
        }

        if rect_vertices.is_empty() {
            return;
        }

        let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scroll Indicator Buffer"),
            contents: bytemuck::cast_slice(&rect_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Scroll Indicator Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scroll Indicator Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.rect_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, rect_buffer.slice(..));
            pass.draw(0..rect_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Render IME preedit text at the cursor position with underline.
    pub fn render_ime_preedit(
        &self,
        view: &wgpu::TextureView,
        preedit_text: &str,
        cursor_x: f32,
        cursor_y: f32,
        cursor_height: f32,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        if preedit_text.is_empty() {
            return;
        }

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let char_width = glyph_atlas.default_font_size() * 0.6;
        let font_size_bits = 0.0_f32.to_bits();
        let text_len = preedit_text.chars().count();
        let preedit_width = text_len as f32 * char_width;

        // Background and underline rects
        let bg_color = Color::new(0.15, 0.15, 0.2, 0.95).srgb_to_linear();
        let underline_color = Color::new(0.4, 0.6, 1.0, 1.0).srgb_to_linear();

        let px = cursor_x;
        let py = cursor_y;
        let pw = preedit_width + 4.0;
        let ph = cursor_height;

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        // Background
        self.add_rect(&mut rect_vertices, px, py, pw, ph, &bg_color);
        // Underline (2px at bottom)
        self.add_rect(&mut rect_vertices, px, py + ph - 2.0, pw, 2.0, &underline_color);

        if !rect_vertices.is_empty() {
            let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("IME Preedit Rect Buffer"),
                contents: bytemuck::cast_slice(&rect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("IME Preedit Rect Encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("IME Preedit Rect Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // Text glyphs
        let text_color = {
            let c = Color::new(1.0, 1.0, 1.0, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();
        for (ci, ch) in preedit_text.chars().enumerate() {
            let key = GlyphKey {
                charcode: ch as u32,
                face_id: 0,
                font_size_bits,
            };
            glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
            overlay_glyphs.push((key, px + 2.0 + (ci as f32) * char_width, py, text_color));
        }
        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    /// Render a visual bell flash overlay (semi-transparent white rectangle fading out).
    /// Render an FPS counter overlay in the top-right corner.
    /// Render a corner mask to clip the window to a rounded rectangle.
    /// Uses dst = dst * src_alpha blend mode to zero out pixels outside
    /// the rounded rect shape.
    pub fn render_corner_mask(
        &self,
        view: &wgpu::TextureView,
        corner_radius: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Filled rounded rect covering the whole frame with alpha=1 inside, 0 outside.
        // border_width=0 triggers filled mode in the shader.
        let mut vertices: Vec<RoundedRectVertex> = Vec::new();
        self.add_rounded_rect(
            &mut vertices,
            0.0, 0.0, logical_w, logical_h,
            0.0,            // border_width=0 → filled mode
            corner_radius,
            &Color::new(1.0, 1.0, 1.0, 1.0), // white, alpha=1
        );

        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Corner Mask Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Corner Mask Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Corner Mask Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.corner_mask_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Build breadcrumb display chars from a file path
    /// Map a file extension to a distinct HSL-based accent color (linear RGB)
    fn extension_to_color(ext: &str) -> (f32, f32, f32) {
        // Well-known extensions get specific colors
        match ext {
            "rs" => (0.8, 0.3, 0.1),  // Rust - orange
            "el" | "lisp" | "scm" => (0.6, 0.2, 0.8),  // Lisp - purple
            "c" | "h" => (0.2, 0.5, 0.8),  // C - blue
            "cpp" | "cc" | "hpp" => (0.2, 0.4, 0.7),  // C++ - darker blue
            "py" => (0.2, 0.6, 0.2),  // Python - green
            "js" | "jsx" => (0.9, 0.8, 0.2),  // JavaScript - yellow
            "ts" | "tsx" => (0.2, 0.5, 0.9),  // TypeScript - blue
            "rb" => (0.8, 0.2, 0.2),  // Ruby - red
            "go" => (0.0, 0.6, 0.7),  // Go - teal
            "java" => (0.7, 0.3, 0.1),  // Java - brown-orange
            "html" | "htm" => (0.9, 0.3, 0.2),  // HTML - red-orange
            "css" | "scss" => (0.2, 0.4, 0.9),  // CSS - blue
            "json" | "yaml" | "yml" | "toml" => (0.5, 0.5, 0.5),  // Config - gray
            "md" | "org" | "txt" => (0.4, 0.7, 0.4),  // Text - green
            "sh" | "bash" | "zsh" => (0.3, 0.7, 0.3),  // Shell - green
            _ => {
                // Hash-based color for unknown extensions
                let mut hash: u32 = 5381;
                for byte in ext.bytes() {
                    hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
                }
                let hue = (hash % 360) as f32 / 360.0;
                // Simple HSL to RGB (saturation=0.6, lightness=0.5)
                let s = 0.6_f32;
                let l = 0.5_f32;
                let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
                let x = c * (1.0 - ((hue * 6.0) % 2.0 - 1.0).abs());
                let m = l - c / 2.0;
                let (r, g, b) = match (hue * 6.0) as i32 {
                    0 => (c, x, 0.0),
                    1 => (x, c, 0.0),
                    2 => (0.0, c, x),
                    3 => (0.0, x, c),
                    4 => (x, 0.0, c),
                    _ => (c, 0.0, x),
                };
                (r + m, g + m, b + m)
            }
        }
    }

    fn breadcrumb_display_chars(path: &str) -> Vec<(char, bool)> {
        let separator = " \u{203A} "; // " › "
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if components.is_empty() {
            return Vec::new();
        }
        let show_start = if components.len() > 3 { components.len() - 3 } else { 0 };
        let shown = &components[show_start..];
        let mut display_chars: Vec<(char, bool)> = Vec::new();
        if show_start > 0 {
            display_chars.push(('\u{2026}', true));
            for c in separator.chars() {
                display_chars.push((c, true));
            }
        }
        for (i, comp) in shown.iter().enumerate() {
            if i > 0 {
                for c in separator.chars() {
                    display_chars.push((c, true));
                }
            }
            let is_last = i == shown.len() - 1;
            for c in comp.chars() {
                display_chars.push((c, !is_last));
            }
        }
        display_chars
    }

    /// Render breadcrumb/path bars for windows with file-backed buffers
    pub fn render_breadcrumbs(
        &mut self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
    ) {
        use wgpu::util::DeviceExt;

        if !self.breadcrumb_enabled {
            return;
        }

        let char_width = glyph_atlas.default_font_size() * 0.6;
        let line_height = glyph_atlas.default_line_height();
        let bar_height = line_height + 4.0;
        let padding_x = 6.0_f32;
        let opacity = self.breadcrumb_opacity.clamp(0.0, 1.0);

        // Detect title changes and start fade animations
        if self.title_fade_enabled {
            for info in &frame_glyphs.window_infos {
                if info.is_minibuffer || info.buffer_file_name.is_empty() {
                    continue;
                }
                let wid = info.window_id;
                let new_text = &info.buffer_file_name;
                let changed = match self.prev_breadcrumb_text.get(&wid) {
                    Some(old) => old != new_text,
                    None => false, // first time seeing this window, no fade
                };
                if changed {
                    let old_text = self.prev_breadcrumb_text.get(&wid).cloned().unwrap_or_default();
                    // Remove any existing fade for this window
                    self.active_title_fades.retain(|f| f.window_id != wid);
                    self.active_title_fades.push(TitleFadeEntry {
                        window_id: wid,
                        bounds: info.bounds,
                        old_text,
                        new_text: new_text.clone(),
                        started: std::time::Instant::now(),
                        duration: std::time::Duration::from_millis(self.title_fade_duration_ms as u64),
                    });
                }
                self.prev_breadcrumb_text.insert(wid, new_text.clone());
            }
            // Clean up expired fades
            self.active_title_fades.retain(|f| f.started.elapsed() < f.duration);
            if !self.active_title_fades.is_empty() {
                self.needs_continuous_redraw = true;
            }
        }

        let mut all_rect_vertices: Vec<RectVertex> = Vec::new();
        let mut all_text_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();
        let font_size_bits = 0.0_f32.to_bits();
        let text_color_base = [0.85_f32, 0.85, 0.85, 1.0];
        let sep_color_base = [0.5_f32, 0.5, 0.5, 1.0];

        for info in &frame_glyphs.window_infos {
            if info.is_minibuffer || info.buffer_file_name.is_empty() {
                continue;
            }

            let b = &info.bounds;

            // Check if this window has an active title fade
            let active_fade = self.active_title_fades.iter().find(|f| f.window_id == info.window_id);

            if let Some(fade) = active_fade {
                // Crossfade: render old text fading out, new text fading in
                let t = (fade.started.elapsed().as_secs_f32() / fade.duration.as_secs_f32()).min(1.0);
                // Ease-out quadratic
                let eased = t * (2.0 - t);
                let new_alpha = eased;
                let old_alpha = 1.0 - eased;

                // Background rect (full opacity)
                let display_chars_new = Self::breadcrumb_display_chars(&info.buffer_file_name);
                let display_chars_old = Self::breadcrumb_display_chars(&fade.old_text);
                let max_len = display_chars_new.len().max(display_chars_old.len());
                let bar_w = (max_len as f32 * char_width + padding_x * 2.0).min(b.width);
                let bar_x = b.x;
                let bar_y = b.y;

                let bg_color = Color::new(0.0, 0.0, 0.0, opacity);
                self.add_rect(&mut all_rect_vertices, bar_x, bar_y, bar_w, bar_height, &bg_color);
                let edge_color = Color::new(0.3, 0.3, 0.3, opacity * 0.5);
                self.add_rect(&mut all_rect_vertices, bar_x, bar_y + bar_height, bar_w, 1.0, &edge_color);

                let text_y = bar_y + 2.0;

                // Old text fading out
                for (ci, &(ch, is_dim)) in display_chars_old.iter().enumerate() {
                    let cx = bar_x + padding_x + ci as f32 * char_width;
                    if cx + char_width > bar_x + bar_w { break; }
                    let key = GlyphKey { charcode: ch as u32, face_id: 0, font_size_bits };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    let base = if is_dim { sep_color_base } else { text_color_base };
                    all_text_glyphs.push((key, cx, text_y,
                        [base[0], base[1], base[2], base[3] * old_alpha]));
                }

                // New text fading in
                for (ci, &(ch, is_dim)) in display_chars_new.iter().enumerate() {
                    let cx = bar_x + padding_x + ci as f32 * char_width;
                    if cx + char_width > bar_x + bar_w { break; }
                    let key = GlyphKey { charcode: ch as u32, face_id: 0, font_size_bits };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    let base = if is_dim { sep_color_base } else { text_color_base };
                    all_text_glyphs.push((key, cx, text_y,
                        [base[0], base[1], base[2], base[3] * new_alpha]));
                }
            } else {
                // Normal rendering (no active fade)
                let display_chars = Self::breadcrumb_display_chars(&info.buffer_file_name);
                if display_chars.is_empty() { continue; }

                let text_width = display_chars.len() as f32 * char_width;
                let bar_w = (text_width + padding_x * 2.0).min(b.width);
                let bar_x = b.x;
                let bar_y = b.y;

                let bg_color = Color::new(0.0, 0.0, 0.0, opacity);
                self.add_rect(&mut all_rect_vertices, bar_x, bar_y, bar_w, bar_height, &bg_color);
                let edge_color = Color::new(0.3, 0.3, 0.3, opacity * 0.5);
                self.add_rect(&mut all_rect_vertices, bar_x, bar_y + bar_height, bar_w, 1.0, &edge_color);

                let text_y = bar_y + 2.0;
                for (ci, &(ch, is_dim)) in display_chars.iter().enumerate() {
                    let cx = bar_x + padding_x + ci as f32 * char_width;
                    if cx + char_width > bar_x + bar_w { break; }
                    let key = GlyphKey { charcode: ch as u32, face_id: 0, font_size_bits };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    all_text_glyphs.push((key, cx, text_y,
                        if is_dim { sep_color_base } else { text_color_base }));
                }
            }
        }

        // Draw background rects
        if !all_rect_vertices.is_empty() {
            let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Breadcrumb Rect Buffer"),
                contents: bytemuck::cast_slice(&all_rect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Breadcrumb Rect Encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Breadcrumb Rect Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..all_rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // Draw text glyphs
        if !all_text_glyphs.is_empty() {
            self.render_overlay_glyphs(view, &mut all_text_glyphs, glyph_atlas);
        }
    }

    /// Render typing speed (WPM) indicator in the bottom-right of the selected window
    pub fn render_typing_speed(
        &self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        wpm: f32,
    ) {
        use wgpu::util::DeviceExt;

        // Find the selected window (non-minibuffer)
        let selected = frame_glyphs.window_infos.iter().find(|w| w.selected && !w.is_minibuffer);
        let info = match selected {
            Some(i) => i,
            None => return,
        };

        let wpm_int = wpm.round() as u32;
        let label = format!("{} WPM", wpm_int);

        let char_width = glyph_atlas.default_font_size() * 0.6;
        let line_height = glyph_atlas.default_line_height();
        let padding_x = 8.0_f32;
        let padding_y = 2.0_f32;
        let bar_w = label.len() as f32 * char_width + padding_x * 2.0;
        let bar_h = line_height + padding_y * 2.0;
        let b = &info.bounds;
        let bar_x = b.x + b.width - bar_w - 4.0;
        // Place just above the mode-line
        let bar_y = b.y + b.height - info.mode_line_height - bar_h - 2.0;

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        let bg_color = Color::new(0.0, 0.0, 0.0, 0.6);
        self.add_rect(&mut rect_vertices, bar_x, bar_y, bar_w, bar_h, &bg_color);

        // Color the label based on WPM: gray→green→yellow→red
        let text_color = if wpm_int == 0 {
            [0.5, 0.5, 0.5, 0.8]
        } else if wpm_int < 40 {
            [0.4, 0.8, 0.4, 1.0] // green
        } else if wpm_int < 80 {
            [0.8, 0.8, 0.2, 1.0] // yellow
        } else {
            [1.0, 0.4, 0.2, 1.0] // orange-red
        };

        let font_size_bits = 0.0_f32.to_bits();
        let mut text_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();
        let text_y = bar_y + padding_y;
        for (ci, ch) in label.chars().enumerate() {
            let cx = bar_x + padding_x + ci as f32 * char_width;
            let key = GlyphKey {
                charcode: ch as u32,
                face_id: 0,
                font_size_bits,
            };
            glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
            text_glyphs.push((key, cx, text_y, text_color));
        }

        // Draw background
        if !rect_vertices.is_empty() {
            let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Typing Speed Rect Buffer"),
                contents: bytemuck::cast_slice(&rect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Typing Speed Rect Encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Typing Speed Rect Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // Draw text
        if !text_glyphs.is_empty() {
            self.render_overlay_glyphs(view, &mut text_glyphs, glyph_atlas);
        }
    }

    pub fn render_fps_overlay(
        &self,
        view: &wgpu::TextureView,
        lines: &[String],
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        if lines.is_empty() {
            return;
        }

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let char_width = glyph_atlas.default_font_size() * 0.6;
        let line_height = glyph_atlas.default_line_height();
        let padding = 4.0_f32;
        let line_spacing = 2.0_f32;

        // Badge size: width = longest line, height = all lines
        let max_text_w = lines.iter()
            .map(|l| l.len() as f32 * char_width)
            .fold(0.0_f32, f32::max);
        let num_lines = lines.len() as f32;
        let badge_w = max_text_w + padding * 2.0;
        let badge_h = num_lines * line_height + (num_lines - 1.0) * line_spacing + padding * 2.0;
        let badge_x = logical_w - badge_w - 4.0;
        let badge_y = 4.0;

        // Background badge (semi-transparent dark)
        let bg = Color::new(0.0, 0.0, 0.0, 0.6);
        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        self.add_rect(&mut rect_vertices, badge_x, badge_y, badge_w, badge_h, &bg);

        let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("FPS Rect Buffer"),
            contents: bytemuck::cast_slice(&rect_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("FPS Rect Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("FPS Rect Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.rect_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, rect_buffer.slice(..));
            pass.draw(0..rect_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));

        // Text glyphs (green for good visibility)
        let text_color = [0.0_f32, 1.0, 0.0, 1.0]; // green in linear
        let font_size_bits = 0.0_f32.to_bits();
        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();
        for (li, line) in lines.iter().enumerate() {
            let y = badge_y + padding + li as f32 * (line_height + line_spacing);
            for (ci, ch) in line.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: 0,
                    font_size_bits,
                };
                glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                overlay_glyphs.push((
                    key,
                    badge_x + padding + ci as f32 * char_width,
                    y,
                    text_color,
                ));
            }
        }
        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    pub fn render_visual_bell(
        &self,
        view: &wgpu::TextureView,
        surface_width: u32,
        surface_height: u32,
        alpha: f32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Semi-transparent white overlay in linear space
        let flash_color = Color::new(1.0, 1.0, 1.0, alpha).srgb_to_linear();

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        self.add_rect(&mut rect_vertices, 0.0, 0.0, logical_w, logical_h, &flash_color);

        let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Visual Bell Buffer"),
            contents: bytemuck::cast_slice(&rect_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Visual Bell Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Visual Bell Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.rect_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, rect_buffer.slice(..));
            pass.draw(0..rect_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }
}
