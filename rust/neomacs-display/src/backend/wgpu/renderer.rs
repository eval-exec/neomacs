//! wgpu GPU-accelerated scene renderer.

use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::core::face::Face;
use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use crate::core::scene::{CursorStyle, Scene};
use crate::core::types::Color;

use super::glyph_atlas::{GlyphKey, WgpuGlyphAtlas};
use super::vertex::{GlyphVertex, RectVertex, Uniforms};

/// GPU-accelerated renderer using wgpu.
pub struct WgpuRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    rect_pipeline: wgpu::RenderPipeline,
    glyph_pipeline: wgpu::RenderPipeline,
    glyph_bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

impl WgpuRenderer {
    /// Create a new WgpuRenderer.
    ///
    /// If a surface is provided, it will be configured for rendering.
    /// Otherwise, the renderer can still be used for offscreen rendering.
    pub fn new(
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
    ) -> Self {
        pollster::block_on(Self::new_async(surface, width, height))
    }

    /// Create a new WgpuRenderer using an existing device and queue.
    ///
    /// This is useful when you need to share the wgpu device with other components,
    /// such as when surfaces are created with a specific device.
    pub fn with_device(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
    ) -> Self {
        Self::create_renderer_internal(device, queue, None, None, width, height)
    }

    /// Internal helper that creates the renderer with the given device/queue.
    ///
    /// This handles pipeline and buffer creation, and is used by both `new_async`
    /// and `with_device`.
    fn create_renderer_internal(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        surface: Option<wgpu::Surface<'static>>,
        surface_config: Option<wgpu::SurfaceConfiguration>,
        width: u32,
        height: u32,
    ) -> Self {
        // Create uniform buffer
        let uniforms = Uniforms {
            screen_size: [width as f32, height as f32],
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
        let target_format = surface_config
            .as_ref()
            .map(|c| c.format)
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

        Self {
            device,
            queue,
            surface,
            surface_config,
            rect_pipeline,
            glyph_pipeline,
            glyph_bind_group_layout,
            uniform_buffer,
            uniform_bind_group,
            width,
            height,
        }
    }

    async fn new_async(
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
    ) -> Self {
        // Create wgpu instance
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Request adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface.as_ref(),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to find a suitable GPU adapter");

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
            .expect("Failed to create device");

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Configure surface if provided
        let surface_config = surface.as_ref().map(|s| {
            let caps = s.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);

            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo, // VSync
                alpha_mode: caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            s.configure(&device, &config);
            config
        });

        // Use the internal helper for pipeline/buffer creation
        Self::create_renderer_internal(device, queue, surface, surface_config, width, height)
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

        // Update uniform buffer
        let uniforms = Uniforms {
            screen_size: [width as f32, height as f32],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
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
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
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
        let color = [color.r, color.g, color.b, color.a];

        let x0 = x;
        let y0 = y;
        let x1 = x + width;
        let y1 = y + height;

        // First triangle (top-left, top-right, bottom-left)
        vertices.push(RectVertex {
            position: [x0, y0],
            color,
        });
        vertices.push(RectVertex {
            position: [x1, y0],
            color,
        });
        vertices.push(RectVertex {
            position: [x0, y1],
            color,
        });

        // Second triangle (top-right, bottom-right, bottom-left)
        vertices.push(RectVertex {
            position: [x1, y0],
            color,
        });
        vertices.push(RectVertex {
            position: [x1, y1],
            color,
        });
        vertices.push(RectVertex {
            position: [x0, y1],
            color,
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

    /// Render frame glyphs to a texture view
    pub fn render_frame_glyphs(
        &self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        faces: &HashMap<u32, Face>,
    ) {
        log::debug!(
            "render_frame_glyphs: {}x{}, {} glyphs, {} faces",
            frame_glyphs.width,
            frame_glyphs.height,
            frame_glyphs.glyphs.len(),
            faces.len()
        );

        // Update uniforms for current frame size
        let uniforms = Uniforms {
            screen_size: [frame_glyphs.width, frame_glyphs.height],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Collect rectangles (backgrounds, stretches, cursors, borders)
        let mut rect_vertices: Vec<RectVertex> = Vec::new();

        // 1. Draw frame background
        self.add_rect(
            &mut rect_vertices,
            0.0,
            0.0,
            frame_glyphs.width,
            frame_glyphs.height,
            &frame_glyphs.background,
        );

        // 2. Process glyphs in order: backgrounds first, then stretches
        for glyph in &frame_glyphs.glyphs {
            match glyph {
                FrameGlyph::Background { bounds, color } => {
                    self.add_rect(
                        &mut rect_vertices,
                        bounds.x,
                        bounds.y,
                        bounds.width,
                        bounds.height,
                        color,
                    );
                }
                FrameGlyph::Stretch {
                    x,
                    y,
                    width,
                    height,
                    bg,
                    ..
                } => {
                    self.add_rect(&mut rect_vertices, *x, *y, *width, *height, bg);
                }
                FrameGlyph::Border {
                    x,
                    y,
                    width,
                    height,
                    color,
                } => {
                    self.add_rect(&mut rect_vertices, *x, *y, *width, *height, color);
                }
                FrameGlyph::Cursor {
                    x,
                    y,
                    width,
                    height,
                    color,
                    ..
                } => {
                    self.add_rect(&mut rect_vertices, *x, *y, *width, *height, color);
                }
                _ => {} // Chars handled separately
            }
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Glyphs Encoder"),
            });

        // Render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Frame Glyphs Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: frame_glyphs.background.r as f64,
                            g: frame_glyphs.background.g as f64,
                            b: frame_glyphs.background.b as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Draw rectangles (backgrounds, stretches, borders, cursors)
            if !rect_vertices.is_empty() {
                let rect_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Rect Vertex Buffer"),
                            contents: bytemuck::cast_slice(&rect_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, rect_buffer.slice(..));
                render_pass.draw(0..rect_vertices.len() as u32, 0..1);
            }

            // Draw character glyphs - collect all vertices first, then batch render
            render_pass.set_pipeline(&self.glyph_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            // First pass: collect glyph keys and vertices (ensures all glyphs are cached)
            let mut glyph_data: Vec<(GlyphKey, [GlyphVertex; 6])> = Vec::new();

            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Char { char, x, y, ascent, fg, face_id, .. } = glyph {
                    let key = GlyphKey {
                        charcode: *char as u32,
                        face_id: *face_id,
                    };

                    let face = faces.get(face_id);

                    if let Some(cached) = glyph_atlas.get_or_create(&self.device, &self.queue, &key, face) {
                        let glyph_x = *x + cached.bearing_x;
                        let glyph_y = *y + *ascent - cached.bearing_y;
                        let glyph_w = cached.width as f32;
                        let glyph_h = cached.height as f32;

                        let vertices = [
                            GlyphVertex { position: [glyph_x, glyph_y], tex_coords: [0.0, 0.0], color: [fg.r, fg.g, fg.b, fg.a] },
                            GlyphVertex { position: [glyph_x + glyph_w, glyph_y], tex_coords: [1.0, 0.0], color: [fg.r, fg.g, fg.b, fg.a] },
                            GlyphVertex { position: [glyph_x + glyph_w, glyph_y + glyph_h], tex_coords: [1.0, 1.0], color: [fg.r, fg.g, fg.b, fg.a] },
                            GlyphVertex { position: [glyph_x, glyph_y], tex_coords: [0.0, 0.0], color: [fg.r, fg.g, fg.b, fg.a] },
                            GlyphVertex { position: [glyph_x + glyph_w, glyph_y + glyph_h], tex_coords: [1.0, 1.0], color: [fg.r, fg.g, fg.b, fg.a] },
                            GlyphVertex { position: [glyph_x, glyph_y + glyph_h], tex_coords: [0.0, 1.0], color: [fg.r, fg.g, fg.b, fg.a] },
                        ];

                        glyph_data.push((key, vertices));
                    }
                }
            }

            log::debug!("render_frame_glyphs: {} char glyphs to render", glyph_data.len());

            // Create single vertex buffer for all glyphs
            if !glyph_data.is_empty() {
                let all_vertices: Vec<GlyphVertex> = glyph_data.iter()
                    .flat_map(|(_, verts)| verts.iter().copied())
                    .collect();

                let glyph_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Glyph Vertex Buffer"),
                    contents: bytemuck::cast_slice(&all_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                render_pass.set_vertex_buffer(0, glyph_buffer.slice(..));

                // Second pass: draw each glyph using cached bind groups
                for (i, (key, _)) in glyph_data.iter().enumerate() {
                    if let Some(cached) = glyph_atlas.get(&key) {
                        render_pass.set_bind_group(1, &cached.bind_group, &[]);
                        let start = (i * 6) as u32;
                        render_pass.draw(start..start + 6, 0..1);
                    }
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
        // Use existing texture pipeline to render webkit content
        // Steps:
        // 1. Create a render pass with the output view
        // 2. Set the texture pipeline (need to add a texture shader)
        // 3. Set the webkit bind group
        // 4. Draw a quad at the specified bounds
    }
}

impl Default for WgpuRenderer {
    fn default() -> Self {
        Self::new(None, 800, 600)
    }
}
