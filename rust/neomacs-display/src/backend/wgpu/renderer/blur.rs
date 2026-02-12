//! Gaussian blur post-processing for the wgpu renderer.
//!
//! Implements a separable two-pass Gaussian blur using ping-pong textures:
//! - Content is first rendered to blur_texture_a (by the caller)
//! - Horizontal blur: read A → write B
//! - Vertical blur: read B → write destination (or back to A for multi-pass)
//!
//! This is fully cross-platform — it's pure wgpu shader code.

use wgpu::util::DeviceExt;
use super::WgpuRenderer;
use super::super::vertex::{BlurUniforms, GlyphVertex};

impl WgpuRenderer {
    /// Ensure blur textures exist and match the current surface dimensions.
    pub(crate) fn ensure_blur_textures(&mut self) {
        let w = self.width;
        let h = self.height;

        // Check if textures already exist with correct size
        if let Some((ref tex, _, _)) = self.blur_texture_a {
            if tex.width() == w && tex.height() == h {
                return;
            }
        }

        log::info!("Creating blur textures: {}x{}", w, h);

        let create_tex = |label: &str| -> (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup) {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.surface_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("{} BG", label)),
                layout: &self.blur_texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.blur_sampler),
                    },
                ],
            });
            (texture, view, bind_group)
        };

        self.blur_texture_a = Some(create_tex("Blur Texture A"));
        self.blur_texture_b = Some(create_tex("Blur Texture B"));
    }

    /// Get the blur render target view (texture A).
    ///
    /// Call `ensure_blur_textures()` first. The caller should render content
    /// to this view, then call `apply_blur_from_a()` to blur it to the final destination.
    pub(crate) fn blur_render_target_view(&self) -> Option<&wgpu::TextureView> {
        self.blur_texture_a.as_ref().map(|(_, v, _)| v)
    }

    /// Apply Gaussian blur from texture A to the destination view.
    ///
    /// Assumes content has already been rendered to blur_texture_a.
    /// Ping-pongs between A and B for each pass (horizontal + vertical).
    /// Final result is written to `dest_view`.
    pub(crate) fn apply_blur_from_a(
        &self,
        dest_view: &wgpu::TextureView,
        passes: u32,
        radius: f32,
    ) {
        if passes == 0 || self.width == 0 || self.height == 0 {
            return;
        }

        let (_, ref view_a, ref bg_a) = self.blur_texture_a.as_ref().expect("blur textures not initialized");
        let (_, ref view_b, ref bg_b) = self.blur_texture_b.as_ref().expect("blur textures not initialized");

        let w = self.width as f32;
        let h = self.height as f32;
        let logical_w = w / self.scale_factor;
        let logical_h = h / self.scale_factor;
        let texel_size = [1.0 / w, 1.0 / h];

        // Build fullscreen quad
        let quad_vertices = [
            GlyphVertex { position: [0.0, 0.0],             tex_coords: [0.0, 0.0], color: [1.0; 4] },
            GlyphVertex { position: [logical_w, 0.0],       tex_coords: [1.0, 0.0], color: [1.0; 4] },
            GlyphVertex { position: [0.0, logical_h],       tex_coords: [0.0, 1.0], color: [1.0; 4] },
            GlyphVertex { position: [logical_w, 0.0],       tex_coords: [1.0, 0.0], color: [1.0; 4] },
            GlyphVertex { position: [logical_w, logical_h], tex_coords: [1.0, 1.0], color: [1.0; 4] },
            GlyphVertex { position: [0.0, logical_h],       tex_coords: [0.0, 1.0], color: [1.0; 4] },
        ];
        let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blur VB"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        for pass in 0..passes {
            let is_last = pass == passes - 1;

            // --- Horizontal blur ---
            // Source: A (pass 0) or result of previous vertical pass
            // Dest: B
            let h_src = if pass % 2 == 0 { bg_a } else { bg_b };
            let h_dst = if pass % 2 == 0 { view_b } else { view_a };

            self.run_blur_pass(&vb, h_src, h_dst, texel_size, [1.0, 0.0], radius);

            // --- Vertical blur ---
            // Source: B (result of horizontal)
            // Dest: surface (last pass) or A (for next iteration)
            let v_src = if pass % 2 == 0 { bg_b } else { bg_a };
            let v_dst = if is_last {
                dest_view
            } else if pass % 2 == 0 {
                view_a
            } else {
                view_b
            };

            self.run_blur_pass(&vb, v_src, v_dst, texel_size, [0.0, 1.0], radius);
        }
    }

    /// Execute a single blur pass (horizontal or vertical).
    fn run_blur_pass(
        &self,
        vb: &wgpu::Buffer,
        src_bind_group: &wgpu::BindGroup,
        dest_view: &wgpu::TextureView,
        texel_size: [f32; 2],
        direction: [f32; 2],
        radius: f32,
    ) {
        let uniforms = BlurUniforms {
            texel_size,
            direction,
            radius,
            _pad: [0.0; 3],
        };
        let ub = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blur Pass UB"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let ubg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur Pass UBG"),
            layout: &self.blur_uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ub.as_entire_binding(),
            }],
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Blur Pass Encoder"),
        });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blur Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dest_view,
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
            rp.set_pipeline(&self.blur_pipeline);
            rp.set_bind_group(0, &self.uniform_bind_group, &[]);
            rp.set_bind_group(1, src_bind_group, &[]);
            rp.set_bind_group(2, &ubg, &[]);
            rp.set_vertex_buffer(0, vb.slice(..));
            rp.draw(0..6, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Free blur textures to reclaim VRAM when blur is disabled.
    pub(crate) fn free_blur_textures(&mut self) {
        self.blur_texture_a = None;
        self.blur_texture_b = None;
    }
}
