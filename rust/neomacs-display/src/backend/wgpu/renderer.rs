//! wgpu GPU-accelerated scene renderer.

use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::core::face::{BoxType, Face, FaceAttributes};
use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use crate::core::scene::{CursorStyle, Scene};
use crate::core::types::{AnimatedCursor, Color};

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
        &self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        faces: &HashMap<u32, Face>,
        surface_width: u32,
        surface_height: u32,
        cursor_visible: bool,
        animated_cursor: Option<AnimatedCursor>,
        mouse_pos: (f32, f32),
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
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Stretch { x, y, width, height, bg, is_overlay, .. } = glyph {
                if !*is_overlay && !overlaps_rounded_box_span(*x, *y, false, &box_spans) {
                    self.add_rect(&mut non_overlay_rect_vertices, *x, *y, *width, *height, bg);
                }
            }
        }
        // Non-overlay char backgrounds (skip boxed chars — they get rounded bg instead)
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Char { x, y, width, height, bg, is_overlay, .. } = glyph {
                if !*is_overlay {
                    if let Some(bg_color) = bg {
                        if !overlaps_rounded_box_span(*x, *y, false, &box_spans) {
                            self.add_rect(&mut non_overlay_rect_vertices, *x, *y, *width, *height, bg_color);
                        }
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
                    // Draw scroll bar track (subtle, slightly transparent)
                    let subtle_track = Color::new(
                        track_color.r, track_color.g, track_color.b,
                        track_color.a * 0.6,
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
                    let effective_thumb = if hovered {
                        Color::new(
                            (thumb_color.r * 1.4).min(1.0),
                            (thumb_color.g * 1.4).min(1.0),
                            (thumb_color.b * 1.4).min(1.0),
                            thumb_color.a.min(1.0),
                        )
                    } else {
                        *thumb_color
                    };

                    // Rounded thumb with pill shape
                    let radius = tw.min(th) * 0.4;
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
                    if *style == 0 {
                        // Filled box cursor: split into bg rect + behind-text trail.
                        // The static cursor bg rect uses cursor_inverse info if available,
                        // otherwise falls back to the cursor color at the static position.
                        if cursor_visible {
                            if let Some(ref inv) = frame_glyphs.cursor_inverse {
                                // Draw cursor bg rect at static position (inverse video background)
                                self.add_rect(&mut cursor_bg_vertices,
                                    inv.x, inv.y, inv.width, inv.height, &inv.cursor_bg);
                            } else {
                                // No inverse info — draw opaque cursor at static position
                                self.add_rect(&mut cursor_bg_vertices, *x, *y, *width, *height, color);
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
                                self.add_quad(&mut behind_text_cursor_vertices, corners, color);
                            } else if let Some(ref anim) = animated_cursor {
                                if *window_id == anim.window_id {
                                    self.add_rect(&mut behind_text_cursor_vertices,
                                        anim.x, anim.y, anim.width, anim.height, color);
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
                                self.add_quad(&mut cursor_vertices, corners, color);
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
                                        self.add_rect(&mut cursor_vertices, cx, cy, 2.0, ch, color);
                                    }
                                    2 => {
                                        // Underline (hbar at bottom)
                                        self.add_rect(&mut cursor_vertices, cx, cy + ch - 2.0, cw, 2.0, color);
                                    }
                                    3 => {
                                        // Hollow box (4 border edges)
                                        self.add_rect(&mut cursor_vertices, cx, cy, cw, 1.0, color);
                                        self.add_rect(&mut cursor_vertices, cx, cy + ch - 1.0, cw, 1.0, color);
                                        self.add_rect(&mut cursor_vertices, cx, cy, 1.0, ch, color);
                                        self.add_rect(&mut cursor_vertices, cx + cw - 1.0, cy, 1.0, ch, color);
                                    }
                                    _ => {
                                        self.add_rect(&mut cursor_vertices, cx, cy, cw, ch, color);
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
                            r: bg.r as f64,
                            g: bg.g as f64,
                            b: bg.b as f64,
                            a: 1.0,
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
                            let glyph_x = *x + cached.bearing_x / sf;
                            let baseline = *y + *ascent;
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
                if !mask_data.is_empty() {
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

                    for (i, (key, _)) in mask_data.iter().enumerate() {
                        if let Some(cached) = glyph_atlas.get(key) {
                            render_pass.set_bind_group(1, &cached.bind_group, &[]);
                            let start = (i * 6) as u32;
                            render_pass.draw(start..start + 6, 0..1);
                        }
                    }
                }

                // Draw color glyphs with image pipeline (direct RGBA, e.g. color emoji)
                if !color_data.is_empty() {
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

                    for (i, (key, _)) in color_data.iter().enumerate() {
                        if let Some(cached) = glyph_atlas.get(key) {
                            render_pass.set_bind_group(1, &cached.bind_group, &[]);
                            let start = (i * 6) as u32;
                            render_pass.draw(start..start + 6, 0..1);
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

                            let baseline_y = *y + *ascent;

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
                                self.add_rect(&mut decoration_vertices, *x, *y, *width, ul_thick.max(1.0), ol_color);
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

        let (mx, my, mw, mh) = menu.bounds;

        // Colors (dark theme popup) — values are in linear space
        // since the surface format (Bgra8UnormSrgb) applies sRGB gamma on output
        let bg_color = Color::new(0.15, 0.15, 0.18, 0.95).srgb_to_linear();
        let border_color = Color::new(0.35, 0.35, 0.40, 1.0).srgb_to_linear();
        let hover_color = Color::new(0.25, 0.40, 0.65, 0.9).srgb_to_linear();
        let text_color = {
            let c = Color::new(0.9, 0.9, 0.9, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let disabled_color = {
            let c = Color::new(0.5, 0.5, 0.5, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let separator_color = Color::new(0.3, 0.3, 0.35, 0.8).srgb_to_linear();
        let title_color = {
            let c = Color::new(0.7, 0.8, 0.9, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let shortcut_color = {
            let c = Color::new(0.6, 0.6, 0.65, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        // === Pass 1: Background rectangles ===
        let mut rect_vertices: Vec<RectVertex> = Vec::new();

        // Drop shadow (layered for soft edge)
        let shadow_layers = 4;
        for i in 1..=shadow_layers {
            let offset = i as f32 * 1.5;
            let alpha = 0.12 * (1.0 - (i - 1) as f32 / shadow_layers as f32);
            let shadow = Color::new(0.0, 0.0, 0.0, alpha);
            self.add_rect(&mut rect_vertices,
                          mx + offset, my + offset, mw, mh, &shadow);
        }

        // Semi-transparent background
        self.add_rect(&mut rect_vertices, mx, my, mw, mh, &bg_color);

        // Border (1px on each side)
        let bw = 1.0_f32;
        self.add_rect(&mut rect_vertices, mx, my, mw, bw, &border_color); // top
        self.add_rect(&mut rect_vertices, mx, my + mh - bw, mw, bw, &border_color); // bottom
        self.add_rect(&mut rect_vertices, mx, my, bw, mh, &border_color); // left
        self.add_rect(&mut rect_vertices, mx + mw - bw, my, bw, mh, &border_color); // right

        // Hover highlight
        if menu.hover_index >= 0 && (menu.hover_index as usize) < menu.items.len() {
            let idx = menu.hover_index as usize;
            let iy = my + menu.item_offsets[idx];
            self.add_rect(&mut rect_vertices, mx + bw, iy, mw - 2.0 * bw, menu.item_height, &hover_color);
        }

        // Separators
        for (i, item) in menu.items.iter().enumerate() {
            if item.separator {
                let iy = my + menu.item_offsets[i] + 3.0;
                self.add_rect(&mut rect_vertices, mx + 8.0, iy, mw - 16.0, 1.0, &separator_color);
            }
        }

        // Title separator
        if menu.title.is_some() {
            let sep_y = my + menu.item_height + 2.0;
            self.add_rect(&mut rect_vertices, mx + 4.0, sep_y, mw - 8.0, 1.0, &separator_color);
        }

        // Submit background rects
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
        let padding = 4.0_f32;
        let font_size = glyph_atlas.default_font_size();
        let char_width = font_size * 0.6;
        // Use default font size (0.0 means use atlas default)
        let font_size_bits = 0.0_f32.to_bits();

        // Render title if present
        if let Some(ref title) = menu.title {
            let tx = mx + padding * 2.0;
            for (ci, ch) in title.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: 0,
                    font_size_bits,
                };
                if let Some(cached) = glyph_atlas.get_or_create(&self.device, &self.queue, &key, None) {
                    let gx = tx + (ci as f32) * char_width;
                    let gy = my + padding;
                    self.render_single_glyph(view, cached, gx, gy, &title_color);
                }
            }
        }

        // Render menu items
        for (i, item) in menu.items.iter().enumerate() {
            if item.separator {
                continue;
            }
            let iy = my + menu.item_offsets[i];
            let color = if !item.enabled {
                &disabled_color
            } else {
                &text_color
            };

            // Render label character by character
            let label_x = mx + padding * 2.0;
            for (ci, ch) in item.label.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: 0,
                    font_size_bits,
                };
                if let Some(cached) = glyph_atlas.get_or_create(&self.device, &self.queue, &key, None) {
                    let gx = label_x + (ci as f32) * char_width;
                    let gy = iy + 2.0; // Small vertical offset within item
                    self.render_single_glyph(view, cached, gx, gy, color);
                }
            }

            // Render shortcut on the right side
            if !item.shortcut.is_empty() {
                let shortcut_x = mx + mw - padding * 2.0 - (item.shortcut.len() as f32 * char_width);
                for (ci, ch) in item.shortcut.chars().enumerate() {
                    let key = GlyphKey {
                        charcode: ch as u32,
                        face_id: 0,
                        font_size_bits,
                    };
                    if let Some(cached) = glyph_atlas.get_or_create(&self.device, &self.queue, &key, None) {
                        let gx = shortcut_x + (ci as f32) * char_width;
                        let gy = iy + 2.0;
                        self.render_single_glyph(view, cached, gx, gy, &shortcut_color);
                    }
                }
            }

            // Render submenu arrow
            if item.submenu {
                let arrow_x = mx + mw - padding * 2.0 - char_width;
                let key = GlyphKey {
                    charcode: '▸' as u32,
                    face_id: 0,
                    font_size_bits,
                };
                if let Some(cached) = glyph_atlas.get_or_create(&self.device, &self.queue, &key, None) {
                    self.render_single_glyph(view, cached, arrow_x, iy + 2.0, &text_color);
                }
            }
        }
    }

    /// Render a single glyph texture at the given position.
    fn render_single_glyph(
        &self,
        view: &wgpu::TextureView,
        cached: &super::glyph_atlas::CachedGlyph,
        x: f32,
        y: f32,
        color: &[f32; 4],
    ) {
        use wgpu::util::DeviceExt;

        let gw = cached.width as f32;
        let gh = cached.height as f32;
        let gx = x + cached.bearing_x;
        let gy = y - cached.bearing_y + 14.0; // Approximate baseline offset

        let vertices = [
            GlyphVertex { position: [gx, gy], tex_coords: [0.0, 0.0], color: *color },
            GlyphVertex { position: [gx + gw, gy], tex_coords: [1.0, 0.0], color: *color },
            GlyphVertex { position: [gx + gw, gy + gh], tex_coords: [1.0, 1.0], color: *color },
            GlyphVertex { position: [gx, gy], tex_coords: [0.0, 0.0], color: *color },
            GlyphVertex { position: [gx + gw, gy + gh], tex_coords: [1.0, 1.0], color: *color },
            GlyphVertex { position: [gx, gy + gh], tex_coords: [0.0, 1.0], color: *color },
        ];

        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Popup Glyph Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let pipeline = if cached.is_color {
            &self.opaque_image_pipeline
        } else {
            &self.image_pipeline
        };

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Popup Glyph Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Popup Glyph Pass"),
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
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_bind_group(1, &cached.bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..6, 0..1);
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

        // === Pass 2: Text glyphs ===
        let padding = 6.0_f32;
        let line_height = glyph_atlas.default_line_height();
        let char_width = glyph_atlas.default_font_size() * 0.6;
        let font_size_bits = 0.0_f32.to_bits();

        for (line_idx, line) in tooltip.lines.iter().enumerate() {
            let ly = ty + padding + line_idx as f32 * line_height;
            for (ci, ch) in line.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: 0,
                    font_size_bits,
                };
                if let Some(cached) = glyph_atlas.get_or_create(&self.device, &self.queue, &key, None) {
                    let gx = tx + padding + (ci as f32) * char_width;
                    self.render_single_glyph(view, cached, gx, ly, &text_color);
                }
            }
        }
    }

    /// Render a visual bell flash overlay (semi-transparent white rectangle fading out).
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
