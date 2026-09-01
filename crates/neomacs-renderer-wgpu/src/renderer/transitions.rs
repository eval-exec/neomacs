//! Transitions methods for WgpuRenderer.

use super::super::vertex::GlyphVertex;
use super::WgpuRenderer;
use neomacs_display_protocol::{
    AxisMotionTransitionEffect, DirectionlessTransitionEffect, HorizontalTransitionEffect,
    ResolvedTransitionEffect, TransitionAxis, TransitionDirection, TransitionEasing,
    TransitionEdge, VerticalPostProcessTransitionEffect, VerticalTransitionEffect,
};

fn slide_offsets(
    axis: TransitionAxis,
    direction: TransitionDirection,
    distance: f32,
    progress: f32,
) -> ([f32; 2], [f32; 2]) {
    let signed_distance = direction.sign() * distance;
    let old = -signed_distance * progress;
    let new = signed_distance * (1.0 - progress);
    match axis {
        TransitionAxis::Horizontal => ([old, 0.0], [new, 0.0]),
        TransitionAxis::Vertical => ([0.0, old], [0.0, new]),
    }
}

fn axis_offset(axis: TransitionAxis, amount: f32) -> [f32; 2] {
    match axis {
        TransitionAxis::Horizontal => [amount, 0.0],
        TransitionAxis::Vertical => [0.0, amount],
    }
}

impl WgpuRenderer {
    /// Render a crossfade transition within a scissor region
    /// Uses the image_pipeline to blend old and new textures
    pub fn render_crossfade(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        blend_t: f32,
        bounds: &neomacs_display_protocol::types::Rect,
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

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Crossfade Encoder"),
            });

        {
            // Pass 1: Draw old texture with alpha (1 - blend_t)
            let old_alpha = 1.0 - blend_t;
            let old_vertices = [
                GlyphVertex {
                    position: [0.0, 0.0],
                    tex_coords: [0.0, 0.0],
                    color: [1.0, 1.0, 1.0, old_alpha],
                },
                GlyphVertex {
                    position: [w, 0.0],
                    tex_coords: [1.0, 0.0],
                    color: [1.0, 1.0, 1.0, old_alpha],
                },
                GlyphVertex {
                    position: [w, h],
                    tex_coords: [1.0, 1.0],
                    color: [1.0, 1.0, 1.0, old_alpha],
                },
                GlyphVertex {
                    position: [0.0, 0.0],
                    tex_coords: [0.0, 0.0],
                    color: [1.0, 1.0, 1.0, old_alpha],
                },
                GlyphVertex {
                    position: [w, h],
                    tex_coords: [1.0, 1.0],
                    color: [1.0, 1.0, 1.0, old_alpha],
                },
                GlyphVertex {
                    position: [0.0, h],
                    tex_coords: [0.0, 1.0],
                    color: [1.0, 1.0, 1.0, old_alpha],
                },
            ];
            let old_upload = self
                .arenas
                .image
                .upload(&self.device, &self.queue, &old_vertices);

            // New texture with alpha blend_t
            let new_vertices = [
                GlyphVertex {
                    position: [0.0, 0.0],
                    tex_coords: [0.0, 0.0],
                    color: [1.0, 1.0, 1.0, blend_t],
                },
                GlyphVertex {
                    position: [w, 0.0],
                    tex_coords: [1.0, 0.0],
                    color: [1.0, 1.0, 1.0, blend_t],
                },
                GlyphVertex {
                    position: [w, h],
                    tex_coords: [1.0, 1.0],
                    color: [1.0, 1.0, 1.0, blend_t],
                },
                GlyphVertex {
                    position: [0.0, 0.0],
                    tex_coords: [0.0, 0.0],
                    color: [1.0, 1.0, 1.0, blend_t],
                },
                GlyphVertex {
                    position: [w, h],
                    tex_coords: [1.0, 1.0],
                    color: [1.0, 1.0, 1.0, blend_t],
                },
                GlyphVertex {
                    position: [0.0, h],
                    tex_coords: [0.0, 1.0],
                    color: [1.0, 1.0, 1.0, blend_t],
                },
            ];
            let new_upload = self
                .arenas
                .image
                .upload(&self.device, &self.queue, &new_vertices);

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Crossfade Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            render_pass.set_scissor_rect(sx, sy, sw, sh);
            render_pass.set_pipeline(&self.pipelines.image);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            // Draw old with fading alpha
            if let Some(ref upload) = old_upload {
                render_pass.set_bind_group(1, old_bind_group, &[]);
                render_pass.set_vertex_buffer(0, upload.buffer_slice());
                render_pass.draw(0..6, 0..1);
            }

            // Draw new with increasing alpha
            if let Some(ref upload) = new_upload {
                render_pass.set_bind_group(1, new_bind_group, &[]);
                render_pass.set_vertex_buffer(0, upload.buffer_slice());
                render_pass.draw(0..6, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a slide transition within a scissor region.
    ///
    /// Uses content-region UV mapping so only the content area of each offscreen
    /// texture is sampled — the mode-line is never included in the sliding quads.
    pub fn render_transition_slide(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        progress: f32,
        axis: TransitionAxis,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        distance: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
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

        // UV coordinates for the content region within the full-frame texture.
        // bounds is already content-only (mode-line excluded by caller).
        let uv_left = bounds.x / w;
        let uv_top = bounds.y / h;
        let uv_right = (bounds.x + bounds.width) / w;
        let uv_bottom = (bounds.y + bounds.height) / h;

        let (old_offset, new_offset) = slide_offsets(axis, direction, distance, progress);

        // Build a content-region quad: position covers the content bounds shifted
        // by y_off, UV maps to exactly the content region in the full-frame texture.
        let make_quad = |offset: [f32; 2]| -> [GlyphVertex; 6] {
            let x0 = bounds.x + offset[0];
            let x1 = bounds.x + bounds.width;
            let x1 = x1 + offset[0];
            let y0 = bounds.y + offset[1];
            let y1 = bounds.y + bounds.height + offset[1];
            [
                GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_left, uv_top],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
                GlyphVertex {
                    position: [x1, y0],
                    tex_coords: [uv_right, uv_top],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
                GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_right, uv_bottom],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
                GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_left, uv_top],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
                GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_right, uv_bottom],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
                GlyphVertex {
                    position: [x0, y1],
                    tex_coords: [uv_left, uv_bottom],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
            ]
        };

        let old_vertices = make_quad(old_offset);
        let new_vertices = make_quad(new_offset);

        let old_upload = self
            .arenas
            .image
            .upload(&self.device, &self.queue, &old_vertices);
        let new_upload = self
            .arenas
            .image
            .upload(&self.device, &self.queue, &new_vertices);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Transition Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Transition Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            render_pass.set_scissor_rect(sx, sy, sw, sh);
            render_pass.set_pipeline(&self.pipelines.image);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            // Draw old texture sliding out
            if let Some(ref upload) = old_upload {
                render_pass.set_bind_group(1, old_bind_group, &[]);
                render_pass.set_vertex_buffer(0, upload.buffer_slice());
                render_pass.draw(0..6, 0..1);
            }

            // Draw new texture sliding in
            if let Some(ref upload) = new_upload {
                render_pass.set_bind_group(1, new_bind_group, &[]);
                render_pass.set_vertex_buffer(0, upload.buffer_slice());
                render_pass.draw(0..6, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render one policy-resolved transition effect.
    ///
    /// This is the main entry point called by `render_transitions()` for each
    /// active transition. It applies the easing function to `raw_t`,
    /// then delegates to the specific effect renderer.
    pub fn render_transition_effect(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        raw_t: f32,
        elapsed_secs: f32,
        bounds: &neomacs_display_protocol::types::Rect,
        effect: ResolvedTransitionEffect,
        easing: TransitionEasing,
        surface_width: u32,
        surface_height: u32,
    ) {
        let eased_t = easing.apply(raw_t);

        match effect {
            ResolvedTransitionEffect::Directionless(DirectionlessTransitionEffect::Crossfade) => {
                self.render_transition_crossfade(
                    surface_view,
                    old_bind_group,
                    new_bind_group,
                    eased_t,
                    bounds,
                    surface_width,
                    surface_height,
                );
            }
            ResolvedTransitionEffect::Directionless(DirectionlessTransitionEffect::ScaleZoom) => {
                self.render_transition_scale_zoom(
                    surface_view,
                    old_bind_group,
                    new_bind_group,
                    eased_t,
                    bounds,
                    surface_width,
                    surface_height,
                );
            }
            ResolvedTransitionEffect::AxisMotion {
                effect: AxisMotionTransitionEffect::Slide,
                axis,
                direction,
                distance,
            } => {
                self.render_transition_slide(
                    surface_view,
                    old_bind_group,
                    new_bind_group,
                    eased_t,
                    axis,
                    direction,
                    bounds,
                    distance,
                    surface_width,
                    surface_height,
                );
            }
            ResolvedTransitionEffect::AxisMotion {
                effect: AxisMotionTransitionEffect::Parallax,
                axis,
                direction,
                distance,
            } => {
                self.render_transition_parallax(
                    surface_view,
                    old_bind_group,
                    new_bind_group,
                    eased_t,
                    axis,
                    direction,
                    bounds,
                    distance,
                    surface_width,
                    surface_height,
                );
            }
            ResolvedTransitionEffect::CardFlip { axis } => {
                self.render_transition_card_flip(
                    surface_view,
                    old_bind_group,
                    new_bind_group,
                    eased_t,
                    axis,
                    bounds,
                    surface_width,
                    surface_height,
                );
            }
            ResolvedTransitionEffect::PageCurl { edge } => {
                self.render_transition_page_curl(
                    surface_view,
                    old_bind_group,
                    new_bind_group,
                    eased_t,
                    edge,
                    bounds,
                    surface_width,
                    surface_height,
                );
            }
            ResolvedTransitionEffect::Vertical {
                effect,
                direction,
                distance,
            } => self.render_vertical_transition_effect(
                surface_view,
                old_bind_group,
                new_bind_group,
                eased_t,
                elapsed_secs,
                direction,
                bounds,
                distance,
                effect,
                surface_width,
                surface_height,
            ),
            ResolvedTransitionEffect::VerticalPostProcess {
                effect,
                direction,
                distance,
            } => self.render_transition_with_post_process(
                surface_view,
                old_bind_group,
                new_bind_group,
                eased_t,
                elapsed_secs,
                direction,
                bounds,
                distance,
                effect,
                surface_width,
                surface_height,
            ),
            ResolvedTransitionEffect::Horizontal {
                effect: HorizontalTransitionEffect::TypewriterReveal,
                direction,
            } => {
                self.render_transition_typewriter(
                    surface_view,
                    old_bind_group,
                    new_bind_group,
                    eased_t,
                    direction,
                    bounds,
                    surface_width,
                    surface_height,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_vertical_transition_effect(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        eased_t: f32,
        elapsed_secs: f32,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        distance: f32,
        effect: VerticalTransitionEffect,
        surface_width: u32,
        surface_height: u32,
    ) {
        match effect {
            VerticalTransitionEffect::FadeEdges => self.render_transition_fade_edges(
                surface_view,
                old_bind_group,
                new_bind_group,
                eased_t,
                direction,
                bounds,
                distance,
                surface_width,
                surface_height,
            ),
            VerticalTransitionEffect::Cascade => self.render_transition_cascade(
                surface_view,
                old_bind_group,
                new_bind_group,
                eased_t,
                elapsed_secs,
                direction,
                bounds,
                distance,
                surface_width,
                surface_height,
            ),
            VerticalTransitionEffect::Tilt => self.render_transition_tilt(
                surface_view,
                old_bind_group,
                new_bind_group,
                eased_t,
                direction,
                bounds,
                distance,
                surface_width,
                surface_height,
            ),
            VerticalTransitionEffect::CylinderRoll => self.render_transition_cylinder_roll(
                surface_view,
                old_bind_group,
                new_bind_group,
                eased_t,
                direction,
                bounds,
                distance,
                surface_width,
                surface_height,
            ),
            VerticalTransitionEffect::Wobbly => self.render_transition_wobbly(
                surface_view,
                old_bind_group,
                new_bind_group,
                eased_t,
                elapsed_secs,
                direction,
                bounds,
                distance,
                surface_width,
                surface_height,
            ),
            VerticalTransitionEffect::Wave => self.render_transition_wave(
                surface_view,
                old_bind_group,
                new_bind_group,
                eased_t,
                elapsed_secs,
                direction,
                bounds,
                distance,
                surface_width,
                surface_height,
            ),
            VerticalTransitionEffect::PerLineSpring => self.render_transition_per_line_spring(
                surface_view,
                old_bind_group,
                new_bind_group,
                eased_t,
                elapsed_secs,
                direction,
                bounds,
                distance,
                surface_width,
                surface_height,
            ),
            VerticalTransitionEffect::Liquid => self.render_transition_liquid(
                surface_view,
                old_bind_group,
                new_bind_group,
                eased_t,
                elapsed_secs,
                direction,
                bounds,
                distance,
                surface_width,
                surface_height,
            ),
        }
    }

    /// Helper: compute scissor rect and content UV from bounds.
    // Returns a flat scissor-rect + UV tuple consumed inline by the caller; a
    // named struct would not be reused elsewhere.
    #[allow(clippy::type_complexity)]
    fn transition_scissor_and_uv(
        &mut self,
        bounds: &neomacs_display_protocol::types::Rect,
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

    /// Helper: upload a GlyphVertex slice into the per-frame image arena.
    fn create_transition_vb(
        &mut self,
        vertices: &[GlyphVertex],
    ) -> Option<super::dynamic_buffer::VertexUpload> {
        self.arenas
            .image
            .upload(&self.device, &self.queue, vertices)
    }

    /// Helper: submit a two-quad transition pass (old + new textures).
    fn submit_transition_two_quad_pass(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        old_vertices: &[GlyphVertex],
        new_vertices: &[GlyphVertex],
        sx: u32,
        sy: u32,
        sw: u32,
        sh: u32,
    ) {
        let old_upload = self.create_transition_vb(old_vertices);
        let new_upload = self.create_transition_vb(new_vertices);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Transition Effect Encoder"),
            });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Transition Effect Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rp.set_scissor_rect(sx, sy, sw, sh);
            rp.set_pipeline(&self.pipelines.image);
            rp.set_bind_group(0, &self.uniform_bind_group, &[]);

            if let Some(ref upload) = old_upload {
                rp.set_bind_group(1, old_bind_group, &[]);
                rp.set_vertex_buffer(0, upload.buffer_slice());
                rp.draw(0..old_vertices.len() as u32, 0..1);
            }

            if let Some(ref upload) = new_upload {
                rp.set_bind_group(1, new_bind_group, &[]);
                rp.set_vertex_buffer(0, upload.buffer_slice());
                rp.draw(0..new_vertices.len() as u32, 0..1);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Crossfade: alpha blend old → new within content bounds.
    fn render_transition_crossfade(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        bounds: &neomacs_display_protocol::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };
        let x0 = bounds.x;
        let y0 = bounds.y;
        let x1 = bounds.x + bounds.width;
        let y1 = bounds.y + bounds.height;
        let old_a = 1.0 - t;

        let old_verts = [
            GlyphVertex {
                position: [x0, y0],
                tex_coords: [uv_l, uv_t],
                color: [1.0, 1.0, 1.0, old_a],
            },
            GlyphVertex {
                position: [x1, y0],
                tex_coords: [uv_r, uv_t],
                color: [1.0, 1.0, 1.0, old_a],
            },
            GlyphVertex {
                position: [x1, y1],
                tex_coords: [uv_r, uv_b],
                color: [1.0, 1.0, 1.0, old_a],
            },
            GlyphVertex {
                position: [x0, y0],
                tex_coords: [uv_l, uv_t],
                color: [1.0, 1.0, 1.0, old_a],
            },
            GlyphVertex {
                position: [x1, y1],
                tex_coords: [uv_r, uv_b],
                color: [1.0, 1.0, 1.0, old_a],
            },
            GlyphVertex {
                position: [x0, y1],
                tex_coords: [uv_l, uv_b],
                color: [1.0, 1.0, 1.0, old_a],
            },
        ];
        let new_verts = [
            GlyphVertex {
                position: [x0, y0],
                tex_coords: [uv_l, uv_t],
                color: [1.0, 1.0, 1.0, t],
            },
            GlyphVertex {
                position: [x1, y0],
                tex_coords: [uv_r, uv_t],
                color: [1.0, 1.0, 1.0, t],
            },
            GlyphVertex {
                position: [x1, y1],
                tex_coords: [uv_r, uv_b],
                color: [1.0, 1.0, 1.0, t],
            },
            GlyphVertex {
                position: [x0, y0],
                tex_coords: [uv_l, uv_t],
                color: [1.0, 1.0, 1.0, t],
            },
            GlyphVertex {
                position: [x1, y1],
                tex_coords: [uv_r, uv_b],
                color: [1.0, 1.0, 1.0, t],
            },
            GlyphVertex {
                position: [x0, y1],
                tex_coords: [uv_l, uv_b],
                color: [1.0, 1.0, 1.0, t],
            },
        ];
        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// ScaleZoom: old shrinks to 95% and fades; new zooms from 95% to 100%.
    fn render_transition_scale_zoom(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        bounds: &neomacs_display_protocol::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
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
                GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, uv_t],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x1, y0],
                    tex_coords: [uv_r, uv_t],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, uv_b],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, uv_t],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, uv_b],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x0, y1],
                    tex_coords: [uv_l, uv_b],
                    color: [1.0, 1.0, 1.0, alpha],
                },
            ]
        };

        let old_verts = make_quad(old_hw, old_hh, old_a);
        let new_verts = make_quad(new_hw, new_hh, t);
        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// FadeEdges: slide with soft fade at viewport top/bottom edges.
    fn render_transition_fade_edges(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        distance: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction.sign();
        let offset = distance * t;
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
                verts.push(GlyphVertex {
                    position: [x0, rel_y0],
                    tex_coords: [uv_l, u0],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x1, rel_y0],
                    tex_coords: [uv_r, u0],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x1, rel_y1],
                    tex_coords: [uv_r, u1],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x0, rel_y0],
                    tex_coords: [uv_l, u0],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x1, rel_y1],
                    tex_coords: [uv_r, u1],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x0, rel_y1],
                    tex_coords: [uv_l, u1],
                    color: c,
                });
            }
            verts
        };

        let old_y_off = -dir * offset;
        let new_y_off = dir * (distance - offset);
        let old_verts = make_strips(old_y_off, true);
        let new_verts = make_strips(new_y_off, false);
        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// Cascade: lines drop in with staggered delay (waterfall).
    fn render_transition_cascade(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        _elapsed_secs: f32,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        distance: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let dir = direction.sign();
        let stagger = 0.06; // 60ms stagger per line

        let make_cascade_strips = |_bind: &wgpu::BindGroup, is_new: bool| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let line_delay = i as f32 * stagger;
                let line_t =
                    ((t - line_delay / 1.0).max(0.0) / (1.0 - line_delay).max(0.01)).min(1.0);
                let eased = 1.0 - (1.0 - line_t).powi(2);

                let base_y = bounds.y + i as f32 * strip_h;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                let (y_off, alpha) = if is_new {
                    (dir * (distance * (1.0 - eased)), eased)
                } else {
                    (-dir * (distance * eased), 1.0 - eased)
                };

                let y0 = base_y + y_off;
                let y1 = base_y + strip_h + y_off;
                let x0 = bounds.x;
                let x1 = bounds.x + bounds.width;
                let c = [1.0, 1.0, 1.0, alpha];

                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, u0],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x1, y0],
                    tex_coords: [uv_r, u0],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, u1],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, u0],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, u1],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x0, y1],
                    tex_coords: [uv_l, u1],
                    color: c,
                });
            }
            verts
        };

        let old_verts = make_cascade_strips(old_bind_group, false);
        let new_verts = make_cascade_strips(new_bind_group, true);
        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// Parallax: layers scroll at different speeds for depth illusion.
    fn render_transition_parallax(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        axis: TransitionAxis,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        distance: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction.sign();
        // Foreground scrolls at normal speed, "background" slower
        // We simulate by having old content move slower (0.7x) and new content normal
        let slow_t = t * 0.7;
        let slow_offset = distance * slow_t;
        let fast_offset = distance * t;

        let make_quad = |offset: [f32; 2], alpha: f32| -> [GlyphVertex; 6] {
            let x0 = bounds.x + offset[0];
            let y0 = bounds.y + offset[1];
            let x1 = bounds.x + bounds.width + offset[0];
            let y1 = bounds.y + bounds.height + offset[1];
            [
                GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, uv_t],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x1, y0],
                    tex_coords: [uv_r, uv_t],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, uv_b],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, uv_t],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, uv_b],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x0, y1],
                    tex_coords: [uv_l, uv_b],
                    color: [1.0, 1.0, 1.0, alpha],
                },
            ]
        };

        let old_verts = make_quad(axis_offset(axis, -dir * slow_offset), 1.0 - t);
        let new_verts = make_quad(axis_offset(axis, dir * (distance - fast_offset)), t);
        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// Tilt: subtle perspective tilt during scroll.
    fn render_transition_tilt(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        distance: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction.sign();
        let offset = distance * t;
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

                verts.push(GlyphVertex {
                    position: [cx - hw0, y0],
                    tex_coords: [uv_l, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [cx + hw0, y0],
                    tex_coords: [uv_r, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [cx + hw1, y1],
                    tex_coords: [uv_r, u1],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [cx - hw0, y0],
                    tex_coords: [uv_l, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [cx + hw1, y1],
                    tex_coords: [uv_r, u1],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [cx - hw1, y1],
                    tex_coords: [uv_l, u1],
                    color: [1.0; 4],
                });
            }
            verts
        };

        let old_verts = make_tilted(-dir * offset);
        let new_verts = make_tilted(dir * (distance - offset));
        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// PageCurl: page curls away revealing new content underneath.
    fn render_transition_page_curl(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        edge: TransitionEdge,
        bounds: &neomacs_display_protocol::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        use neomacs_display_protocol::scroll_animation::page_curl_transform;
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let num_strips = 24;

        // New content: flat, full opacity (drawn first, underneath)
        let new_verts: Vec<GlyphVertex> = {
            let x0 = bounds.x;
            let x1 = bounds.x + bounds.width;
            let y0 = bounds.y;
            let y1 = bounds.y + bounds.height;
            vec![
                GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, uv_t],
                    color: [1.0; 4],
                },
                GlyphVertex {
                    position: [x1, y0],
                    tex_coords: [uv_r, uv_t],
                    color: [1.0; 4],
                },
                GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, uv_b],
                    color: [1.0; 4],
                },
                GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, uv_t],
                    color: [1.0; 4],
                },
                GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, uv_b],
                    color: [1.0; 4],
                },
                GlyphVertex {
                    position: [x0, y1],
                    tex_coords: [uv_l, uv_b],
                    color: [1.0; 4],
                },
            ]
        };

        // Old content curls from the policy-selected edge. Horizontal edges
        // tessellate columns; vertical edges tessellate rows, so content is
        // deformed without rotating its texture.
        let old_verts: Vec<GlyphVertex> = {
            let mut verts = Vec::with_capacity(num_strips * 6);
            let mut append_quad = |x0: f32,
                                   y0: f32,
                                   x1: f32,
                                   y1: f32,
                                   u0: f32,
                                   v0: f32,
                                   u1: f32,
                                   v1: f32,
                                   alpha: f32| {
                let c = [1.0, 1.0, 1.0, alpha];
                verts.extend_from_slice(&[
                    GlyphVertex {
                        position: [x0, y0],
                        tex_coords: [u0, v0],
                        color: c,
                    },
                    GlyphVertex {
                        position: [x1, y0],
                        tex_coords: [u1, v0],
                        color: c,
                    },
                    GlyphVertex {
                        position: [x1, y1],
                        tex_coords: [u1, v1],
                        color: c,
                    },
                    GlyphVertex {
                        position: [x0, y0],
                        tex_coords: [u0, v0],
                        color: c,
                    },
                    GlyphVertex {
                        position: [x1, y1],
                        tex_coords: [u1, v1],
                        color: c,
                    },
                    GlyphVertex {
                        position: [x0, y1],
                        tex_coords: [u0, v1],
                        color: c,
                    },
                ]);
            };
            for i in 0..num_strips {
                let nt = i as f32 / num_strips as f32;
                match edge {
                    TransitionEdge::Left | TransitionEdge::Right => {
                        let from_positive = edge == TransitionEdge::Right;
                        let curl_pos = if from_positive { nt } else { 1.0 - nt };
                        let (cross_off, axial_off, alpha) =
                            page_curl_transform(curl_pos, t, bounds.width);
                        let axial_sign = if from_positive { 1.0 } else { -1.0 };
                        let strip_w = bounds.width / num_strips as f32;
                        let uv_strip_w = (uv_r - uv_l) / num_strips as f32;
                        let x0 = bounds.x + i as f32 * strip_w + axial_sign * axial_off;
                        let x1 = bounds.x + (i + 1) as f32 * strip_w + axial_sign * axial_off;
                        let y0 = bounds.y + cross_off;
                        let y1 = bounds.y + bounds.height + cross_off;
                        let u0 = uv_l + i as f32 * uv_strip_w;
                        let u1 = uv_l + (i + 1) as f32 * uv_strip_w;
                        append_quad(x0, y0, x1, y1, u0, uv_t, u1, uv_b, alpha);
                    }
                    TransitionEdge::Top | TransitionEdge::Bottom => {
                        let from_positive = edge == TransitionEdge::Bottom;
                        let curl_pos = if from_positive { nt } else { 1.0 - nt };
                        let (cross_off, axial_off, alpha) =
                            page_curl_transform(curl_pos, t, bounds.height);
                        let axial_sign = if from_positive { 1.0 } else { -1.0 };
                        let strip_h = bounds.height / num_strips as f32;
                        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
                        let x0 = bounds.x + cross_off;
                        let x1 = bounds.x + bounds.width + cross_off;
                        let y0 = bounds.y + i as f32 * strip_h + axial_sign * axial_off;
                        let y1 = bounds.y + (i + 1) as f32 * strip_h + axial_sign * axial_off;
                        let v0 = uv_t + i as f32 * uv_strip_h;
                        let v1 = uv_t + (i + 1) as f32 * uv_strip_h;
                        append_quad(x0, y0, x1, y1, uv_l, v0, uv_r, v1, alpha);
                    }
                }
            }
            verts
        };

        // Draw new first (underneath), then old (curling on top)
        self.submit_transition_two_quad_pass(
            surface_view,
            new_bind_group,
            old_bind_group,
            &new_verts,
            &old_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// CardFlip: screenful flips like a card around X-axis.
    fn render_transition_card_flip(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        axis: TransitionAxis,
        bounds: &neomacs_display_protocol::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let cx = bounds.x + bounds.width / 2.0;
        let cy = bounds.y + bounds.height / 2.0;

        // A horizontal transition rotates around the Y axis and therefore
        // shrinks width; a vertical transition rotates around the X axis and
        // shrinks height.
        let angle = t * std::f32::consts::PI;
        let scale = angle.cos().abs().max(0.02);
        let (hw, hh) = match axis {
            TransitionAxis::Horizontal => (bounds.width / 2.0 * scale, bounds.height / 2.0),
            TransitionAxis::Vertical => (bounds.width / 2.0, bounds.height / 2.0 * scale),
        };

        let (bind_group, alpha) = if t < 0.5 {
            (old_bind_group, 1.0)
        } else {
            (new_bind_group, 1.0)
        };

        let verts = [
            GlyphVertex {
                position: [cx - hw, cy - hh],
                tex_coords: [uv_l, uv_t],
                color: [1.0, 1.0, 1.0, alpha],
            },
            GlyphVertex {
                position: [cx + hw, cy - hh],
                tex_coords: [uv_r, uv_t],
                color: [1.0, 1.0, 1.0, alpha],
            },
            GlyphVertex {
                position: [cx + hw, cy + hh],
                tex_coords: [uv_r, uv_b],
                color: [1.0, 1.0, 1.0, alpha],
            },
            GlyphVertex {
                position: [cx - hw, cy - hh],
                tex_coords: [uv_l, uv_t],
                color: [1.0, 1.0, 1.0, alpha],
            },
            GlyphVertex {
                position: [cx + hw, cy + hh],
                tex_coords: [uv_r, uv_b],
                color: [1.0, 1.0, 1.0, alpha],
            },
            GlyphVertex {
                position: [cx - hw, cy + hh],
                tex_coords: [uv_l, uv_b],
                color: [1.0, 1.0, 1.0, alpha],
            },
        ];

        // Single texture pass
        let empty: [GlyphVertex; 0] = [];
        self.submit_transition_two_quad_pass(
            surface_view,
            bind_group,
            bind_group,
            &verts,
            &empty,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// CylinderRoll: content wraps around a vertical cylinder.
    fn render_transition_cylinder_roll(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        distance: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let num_strips = 16;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;
        let dir = direction.sign();
        let offset = distance * t;
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
                let alpha = if is_old {
                    (1.0 - t) * brightness
                } else {
                    t * brightness
                };

                let y0 = bounds.y + i as f32 * strip_h + y_base_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;
                let c = [brightness, brightness, brightness, alpha];

                verts.push(GlyphVertex {
                    position: [cx - hw, y0],
                    tex_coords: [uv_l, u0],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [cx + hw, y0],
                    tex_coords: [uv_r, u0],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [cx + hw, y1],
                    tex_coords: [uv_r, u1],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [cx - hw, y0],
                    tex_coords: [uv_l, u0],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [cx + hw, y1],
                    tex_coords: [uv_r, u1],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [cx - hw, y1],
                    tex_coords: [uv_l, u1],
                    color: c,
                });
            }
            verts
        };

        let old_verts = make_cylinder(-dir * offset, true);
        let new_verts = make_cylinder(dir * (distance - offset), false);
        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// Wobbly/jelly: content deforms elastically during scroll.
    fn render_transition_wobbly(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        _elapsed_secs: f32,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        distance: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        use neomacs_display_protocol::scroll_animation::wobbly_deform;
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction.sign();
        let offset = distance * t;
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

                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x1, y0],
                    tex_coords: [uv_r, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, u1],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, u1],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x0, y1],
                    tex_coords: [uv_l, u1],
                    color: [1.0; 4],
                });
            }
            verts
        };

        let old_verts = make_wobbly(-dir * offset);
        let new_verts = make_wobbly(dir * (distance - offset));
        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// Wave: horizontal sine-wave displacement during scroll.
    fn render_transition_wave(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        distance: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction.sign();
        let offset = distance * t;
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

                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x1, y0],
                    tex_coords: [uv_r, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, u1],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, u1],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x0, y1],
                    tex_coords: [uv_l, u1],
                    color: [1.0; 4],
                });
            }
            verts
        };

        let old_verts = make_wave(-dir * offset);
        let new_verts = make_wave(dir * (distance - offset));
        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// PerLineSpring: each line on own spring with stagger delay.
    fn render_transition_per_line_spring(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        _t: f32,
        elapsed_secs: f32,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        distance: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction.sign();
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

                let line_offset = distance * spring_t;
                let y_off = if is_new {
                    dir * (distance - line_offset)
                } else {
                    -dir * line_offset
                };

                let x0 = bounds.x;
                let x1 = bounds.x + bounds.width;
                let y0 = bounds.y + i as f32 * strip_h + y_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;

                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x1, y0],
                    tex_coords: [uv_r, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, u1],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, u1],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x0, y1],
                    tex_coords: [uv_l, u1],
                    color: [1.0; 4],
                });
            }
            verts
        };

        let old_verts = make_spring(false);
        let new_verts = make_spring(true);
        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// Liquid: noise-based UV warping, text ripples like water.
    fn render_transition_liquid(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        elapsed_secs: f32,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        distance: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        use neomacs_display_protocol::scroll_animation::liquid_deform;
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction.sign();
        let offset = distance * t;
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

                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x1, y0],
                    tex_coords: [uv_r, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, u1],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, u0],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, u1],
                    color: [1.0; 4],
                });
                verts.push(GlyphVertex {
                    position: [x0, y1],
                    tex_coords: [uv_l, u1],
                    color: [1.0; 4],
                });
            }
            verts
        };

        let old_verts = make_liquid(-dir * offset);
        let new_verts = make_liquid(dir * (distance - offset));
        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// Post-process a vertical transition with color and alpha modulation.
    ///
    /// Since we don't have a separate post-process shader pipeline yet, we approximate
    /// post-processing effects by manipulating vertex colors during the slide transition.
    fn render_transition_with_post_process(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        eased_t: f32,
        elapsed_secs: f32,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        distance: f32,
        effect: VerticalPostProcessTransitionEffect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

        let dir = direction.sign();
        let offset = distance * eased_t;
        let speed = 1.0 - eased_t; // High at start, low at end
        let num_strips = 20;
        let strip_h = bounds.height / num_strips as f32;
        let uv_strip_h = (uv_b - uv_t) / num_strips as f32;

        let make_postprocess = |y_base_off: f32, _is_old: bool| -> Vec<GlyphVertex> {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let nt = i as f32 / num_strips as f32;
                let nt_center = (nt - 0.5).abs() * 2.0; // 0 at center, 1 at edges

                let mut r = 1.0_f32;
                let mut g = 1.0_f32;
                let mut b = 1.0_f32;
                let mut alpha = 1.0_f32;
                let dx = 0.0_f32;

                match effect {
                    VerticalPostProcessTransitionEffect::MotionBlur => {
                        // Simulate blur by reducing alpha at edges proportional to speed
                        let blur = speed * 0.4;
                        alpha = 1.0 - nt_center * blur;
                    }
                    VerticalPostProcessTransitionEffect::ChromaticAberration => {
                        // Shift color channels based on position and speed
                        let shift = speed * 0.08;
                        r = 1.0 + shift * (nt - 0.5);
                        b = 1.0 - shift * (nt - 0.5);
                    }
                    VerticalPostProcessTransitionEffect::GhostTrails => {
                        // Reduced alpha creates ghost-like transparency
                        let ghost = speed * 0.3;
                        alpha = 1.0 - ghost * nt_center;
                    }
                    VerticalPostProcessTransitionEffect::ColorTemperature => {
                        // Warm (orange) scrolling down, cool (blue) scrolling up
                        let temp = dir * speed * 0.06;
                        r = (1.0 + temp).clamp(0.9, 1.1);
                        b = (1.0 - temp).clamp(0.9, 1.1);
                    }
                    VerticalPostProcessTransitionEffect::CRTScanlines => {
                        // Scanline brightness modulation
                        let scanline =
                            (nt * num_strips as f32 * 2.0 + elapsed_secs * 20.0).sin() * 0.5 + 0.5;
                        let intensity = 1.0 - speed * 0.15 * scanline;
                        r = intensity;
                        g = intensity;
                        b = intensity;
                    }
                    VerticalPostProcessTransitionEffect::DepthOfField => {
                        // Edges get dimmer (simulating blur)
                        let dof = speed * 0.3;
                        let brightness = 1.0 - nt_center * dof;
                        r = brightness;
                        g = brightness;
                        b = brightness;
                    }
                }

                let x0 = bounds.x + dx;
                let x1 = bounds.x + bounds.width + dx;
                let y0 = bounds.y + i as f32 * strip_h + y_base_off;
                let y1 = bounds.y + (i + 1) as f32 * strip_h + y_base_off;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;
                let c = [r, g, b, alpha];

                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, u0],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x1, y0],
                    tex_coords: [uv_r, u0],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, u1],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, u0],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, u1],
                    color: c,
                });
                verts.push(GlyphVertex {
                    position: [x0, y1],
                    tex_coords: [uv_l, u1],
                    color: c,
                });
            }
            verts
        };

        let old_verts = make_postprocess(-dir * offset, true);
        let new_verts = make_postprocess(dir * (distance - offset), false);
        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }

    /// TypewriterReveal: new lines appear character-by-character (simulated with strips).
    fn render_transition_typewriter(
        &mut self,
        surface_view: &wgpu::TextureView,
        old_bind_group: &wgpu::BindGroup,
        new_bind_group: &wgpu::BindGroup,
        t: f32,
        direction: TransitionDirection,
        bounds: &neomacs_display_protocol::types::Rect,
        surface_width: u32,
        surface_height: u32,
    ) {
        let (sx, sy, sw, sh, _w, _h, uv_l, uv_t, uv_r, uv_b) =
            match self.transition_scissor_and_uv(bounds, surface_width, surface_height) {
                Some(v) => v,
                None => return,
            };

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
                GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, uv_t],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x1, y0],
                    tex_coords: [uv_r, uv_t],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, uv_b],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [uv_l, uv_t],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [uv_r, uv_b],
                    color: [1.0, 1.0, 1.0, alpha],
                },
                GlyphVertex {
                    position: [x0, y1],
                    tex_coords: [uv_l, uv_b],
                    color: [1.0, 1.0, 1.0, alpha],
                },
            ]
        };

        // New content: each line reveals left-to-right with stagger
        let new_verts: Vec<GlyphVertex> = {
            let mut verts = Vec::with_capacity(num_strips * 6);
            for i in 0..num_strips {
                let line_delay = i as f32 * stagger;
                let line_t = ((t - line_delay).max(0.0) / (1.0 - line_delay).max(0.01)).min(1.0);

                let reveal = line_t;
                let reveal_width = bounds.width * reveal;
                let uv_reveal_width = (uv_r - uv_l) * reveal;
                let (x0, x1, reveal_u0, reveal_u1) = match direction {
                    TransitionDirection::Forward => (
                        bounds.x,
                        bounds.x + reveal_width,
                        uv_l,
                        uv_l + uv_reveal_width,
                    ),
                    TransitionDirection::Backward => (
                        bounds.x + bounds.width - reveal_width,
                        bounds.x + bounds.width,
                        uv_r - uv_reveal_width,
                        uv_r,
                    ),
                };

                let y0 = bounds.y + i as f32 * strip_h;
                let y1 = bounds.y + (i + 1) as f32 * strip_h;
                let u0 = uv_t + i as f32 * uv_strip_h;
                let u1 = uv_t + (i + 1) as f32 * uv_strip_h;
                let alpha = line_t;

                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [reveal_u0, u0],
                    color: [1.0, 1.0, 1.0, alpha],
                });
                verts.push(GlyphVertex {
                    position: [x1, y0],
                    tex_coords: [reveal_u1, u0],
                    color: [1.0, 1.0, 1.0, alpha],
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [reveal_u1, u1],
                    color: [1.0, 1.0, 1.0, alpha],
                });
                verts.push(GlyphVertex {
                    position: [x0, y0],
                    tex_coords: [reveal_u0, u0],
                    color: [1.0, 1.0, 1.0, alpha],
                });
                verts.push(GlyphVertex {
                    position: [x1, y1],
                    tex_coords: [reveal_u1, u1],
                    color: [1.0, 1.0, 1.0, alpha],
                });
                verts.push(GlyphVertex {
                    position: [x0, y1],
                    tex_coords: [reveal_u0, u1],
                    color: [1.0, 1.0, 1.0, alpha],
                });
            }
            verts
        };

        self.submit_transition_two_quad_pass(
            surface_view,
            old_bind_group,
            new_bind_group,
            &old_verts,
            &new_verts,
            sx,
            sy,
            sw,
            sh,
        );
    }
}

#[cfg(test)]
#[path = "transitions_test.rs"]
mod tests;
