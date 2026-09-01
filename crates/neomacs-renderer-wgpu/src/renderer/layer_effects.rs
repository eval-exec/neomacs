//! Effect overlay stacks drawn before and after the frame content:
//! the pre-content background effects (step 1a/1b), the core window/cursor
//! effect stack, the extended decorative stack, and the post-content
//! chrome/focus/highlight/navigation/input/overlay stacks.
//!
//! Every stack is an ordered display list of `draw_effect!`/`draw_stateful!`
//! invocations; the order is the z-order and must not be reordered.

use neomacs_display_protocol::types::FaceId;
use std::collections::HashMap;

use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::types::Color;

use super::super::vertex::{RectVertex, RoundedRectVertex};
use super::WgpuRenderer;
use super::frame_pass::BoxSpan;

/// Draw effect vertices produced by a pure effect function.
macro_rules! draw_effect {
    ($self:ident, $rp:ident, $label:expr, $verts:expr) => {{
        let verts = $verts;
        if let Some(upload) = $self
            .arenas
            .rect
            .upload(&$self.device, &$self.queue, &verts)
        {
            $rp.set_pipeline(&$self.pipelines.rect);
            $rp.set_bind_group(0, &$self.uniform_bind_group, &[]);
            $rp.set_vertex_buffer(0, upload.buffer_slice());
            $rp.draw(0..verts.len() as u32, 0..1);
        }
    }};
    // Animated/time-based effects. Continuation is no longer latched here: the
    // effect's own state is polled by RendererFrameEffects::needs_redraw, so
    // the `continuous` marker is retained only to keep call sites readable.
    ($self:ident, $rp:ident, $label:expr, $verts:expr, continuous) => {{
        let verts = $verts;
        if !verts.is_empty() {
            if let Some(upload) = $self
                .arenas
                .rect
                .upload(&$self.device, &$self.queue, &verts)
            {
                $rp.set_pipeline(&$self.pipelines.rect);
                $rp.set_bind_group(0, &$self.uniform_bind_group, &[]);
                $rp.set_vertex_buffer(0, upload.buffer_slice());
                $rp.draw(0..verts.len() as u32, 0..1);
            }
        }
    }};
}

/// Draw effect vertices from a stateful effect function that returns
/// `(Vec<RectVertex>, bool)`. The bool (whether the effect wants another
/// frame) is ignored: continuation is polled from effect state by
/// `needs_redraw`, not latched during drawing.
macro_rules! draw_stateful {
    ($self:ident, $rp:ident, $label:expr, $result:expr) => {{
        let (verts, _needs_redraw) = $result;
        if let Some(upload) = $self
            .arenas
            .rect
            .upload(&$self.device, &$self.queue, &verts)
        {
            $rp.set_pipeline(&$self.pipelines.rect);
            $rp.set_bind_group(0, &$self.uniform_bind_group, &[]);
            $rp.set_vertex_buffer(0, upload.buffer_slice());
            $rp.draw(0..verts.len() as u32, 0..1);
        }
    }};
}

impl WgpuRenderer {
    pub(super) fn draw_pre_content_background_effects(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        ctx: &super::effect_common::EffectCtx<'_>,
        faces: &HashMap<FaceId, Face>,
        box_spans: &[BoxSpan],
    ) {
        // === Step 1a: Background pattern (dots/grid/crosshatch) ===
        draw_effect!(
            self,
            render_pass,
            "Background Pattern",
            super::pattern_effects::emit_background_pattern(ctx)
        );

        // === Step 1b: Draw filled rounded rect backgrounds for ROUNDED boxed spans ===
        // Only for corner_radius > 0. Standard boxes use normal rect backgrounds.
        let mut required_box_fill_vertices: Vec<RectVertex> = Vec::new();
        let mut box_fill_vertices: Vec<RoundedRectVertex> = Vec::new();
        for span in box_spans {
            if span.row_role.is_chrome() {
                continue;
            }
            if let Some(face) = faces.get(&span.face_id) {
                self.append_required_box_background_fill_geometry(
                    &mut required_box_fill_vertices,
                    span,
                    0.0,
                    0.0,
                );
                self.append_rounded_box_fill_geometry(&mut box_fill_vertices, span, face, 0.0, 0.0);
            }
        }
        self.draw_rect_vertex_layer(render_pass, &required_box_fill_vertices);
        if let Some(upload) =
            self.arenas
                .rounded
                .upload(&self.device, &self.queue, &box_fill_vertices)
        {
            render_pass.set_pipeline(&self.pipelines.rounded_rect);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, upload.buffer_slice());
            render_pass.draw(0..box_fill_vertices.len() as u32, 0..1);
        }
    }

    /// Draw the pre-content effect stacks: the core window/cursor effects,
    /// then the extended decorative effects.
    pub(super) fn draw_pre_content_effects(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        ctx: &super::effect_common::EffectCtx<'_>,
    ) {
        self.draw_pre_content_effects_core(render_pass, ctx);
        self.draw_pre_content_effects_extended(render_pass, ctx);
    }

    /// Core window and cursor effect stack: cursor guides, window framing,
    /// mode-line accents, and cursor/ambient trails, in fixed z-order.
    fn draw_pre_content_effects_core(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        ctx: &super::effect_common::EffectCtx<'_>,
    ) {
        // === Step 1c: Cursor glow ===
        draw_effect!(
            self,
            render_pass,
            "Cursor Glow",
            super::cursor_effects::emit_cursor_glow(ctx, &self.clocks.cursor_pulse_start)
        );

        // === Step 1d: Draw cursor crosshair guide lines ===
        draw_effect!(
            self,
            render_pass,
            "Cursor Crosshair",
            super::cursor_effects::emit_cursor_crosshair(ctx)
        );

        // === Step 1e: Draw buffer modified border indicator ===
        draw_effect!(
            self,
            render_pass,
            "Modified Indicator",
            super::window_effects::emit_modified_indicator(ctx)
        );

        // === Step 1f: Typing heat map overlay ===
        draw_stateful!(
            self,
            render_pass,
            "Heat Map",
            super::window_effects::emit_typing_heatmap(
                ctx,
                &mut self.fx.typing_heatmap.entries,
                &mut self.fx.typing_heatmap.prev_cursor
            )
        );

        // === Step 1g: Per-window rounded border ===
        self.draw_window_border_radius_effect(render_pass, ctx);

        // === Step 1h: Inactive window stained glass effect ===
        draw_effect!(
            self,
            render_pass,
            "Stained Glass",
            super::window_effects::emit_stained_glass(ctx)
        );

        // === Step 1i_focus: Focus gradient border ===
        draw_effect!(
            self,
            render_pass,
            "Focus Gradient Border",
            super::window_effects::emit_focus_gradient_border(ctx)
        );

        // === Step 1i_depth: Window depth shadow layers ===
        draw_effect!(
            self,
            render_pass,
            "Depth Shadow",
            super::window_effects::emit_window_depth_shadow(ctx)
        );

        // === Step 1i_modeline_grad: Mode-line gradient background ===
        draw_effect!(
            self,
            render_pass,
            "Mode-line Gradient",
            super::window_effects::emit_mode_line_gradient(ctx)
        );

        // === Step 1i_magnetism: Cursor magnetism effect ===
        draw_stateful!(
            self,
            render_pass,
            "Cursor Magnetism",
            super::cursor_effects::emit_cursor_magnetism(
                ctx,
                &mut self.fx.cursor_magnetism.entries
            )
        );

        // === Step 1i2: Window corner fold effect ===
        draw_effect!(
            self,
            render_pass,
            "Corner Fold",
            super::window_effects::emit_window_corner_fold(ctx)
        );

        // === Step 1i2: Frosted window border effect ===
        draw_effect!(
            self,
            render_pass,
            "Frosted Border",
            super::window_effects::emit_frosted_window_border(ctx)
        );

        // === Step 1i3: Line number pulse on cursor line ===
        draw_stateful!(
            self,
            render_pass,
            "Line Number Pulse",
            super::cursor_effects::emit_line_number_pulse(ctx)
        );

        // === Step 1i4: Window breathing border animation ===
        draw_stateful!(
            self,
            render_pass,
            "Breathing Border",
            super::window_effects::emit_window_breathing_border(ctx)
        );

        // === Step 1i5: Window scanline (CRT) effect ===
        draw_effect!(
            self,
            render_pass,
            "Scanlines",
            super::window_effects::emit_window_scanline(ctx)
        );

        // === Step 1j: Cursor spotlight/radial gradient effect ===
        draw_effect!(
            self,
            render_pass,
            "Cursor Spotlight",
            super::cursor_effects::emit_cursor_spotlight(ctx)
        );

        // === Step 1k: Cursor comet tail effect ===
        draw_stateful!(
            self,
            render_pass,
            "Cursor Comet",
            super::cursor_effects::emit_cursor_comet(ctx, &mut self.fx.cursor_comet.positions)
        );

        // === Step 1l: Cursor particle trail effect ===
        draw_stateful!(
            self,
            render_pass,
            "Cursor Particles",
            super::cursor_effects::emit_cursor_particles(
                ctx,
                &mut self.fx.cursor_particles.entries,
                &mut self.fx.cursor_particles.prev_pos
            )
        );

        // Matrix/digital rain effect
        draw_stateful!(
            self,
            render_pass,
            "Matrix Rain",
            super::cursor_effects::emit_matrix_rain(ctx, &mut self.fx.matrix_rain.columns)
        );

        // Frost/ice border effect
        draw_effect!(
            self,
            render_pass,
            "Frost Border",
            super::cursor_effects::emit_frost_border(ctx)
        );

        // Cursor ghost afterimage effect
        draw_stateful!(
            self,
            render_pass,
            "Cursor Ghost",
            super::window_effects::emit_cursor_ghost(ctx, &mut self.fx.cursor_ghost.entries)
        );

        // Edge glow on scroll boundaries
        draw_stateful!(
            self,
            render_pass,
            "Edge Glow",
            super::window_effects::emit_edge_glow(ctx, &mut self.fx.edge_glow.entries)
        );

        // Rain/drip ambient effect
        draw_stateful!(
            self,
            render_pass,
            "Rain",
            super::window_effects::emit_rain_effect(ctx, &mut self.fx.rain.drops)
        );

        // Cursor ripple wave effect
        draw_stateful!(
            self,
            render_pass,
            "Cursor Ripple",
            super::cursor_effects::emit_cursor_ripple_wave(ctx, &mut self.fx.ripple_wave.waves)
        );
    }

    fn draw_window_border_radius_effect(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        ctx: &super::effect_common::EffectCtx<'_>,
    ) {
        if !self.effects.window_border_radius.enabled {
            return;
        }

        let (wr, wg, wb) = self.effects.window_border_radius.color;
        let walpha = self.effects.window_border_radius.opacity;
        let wc = Color::new(wr, wg, wb, walpha);
        let radius = self.effects.window_border_radius.radius;
        let bw = self.effects.window_border_radius.width;
        let mut border_verts: Vec<RoundedRectVertex> = Vec::new();
        for win_info in &ctx.frame_glyphs.window_infos {
            if !win_info.is_minibuffer {
                let wb_bounds = &win_info.bounds;
                let mode_h = win_info.mode_line_height;
                let content_h = wb_bounds.height - mode_h;
                if content_h > 0.0 {
                    self.add_rounded_rect(
                        &mut border_verts,
                        wb_bounds.x,
                        wb_bounds.y,
                        wb_bounds.width,
                        content_h,
                        bw,
                        radius,
                        &wc,
                    );
                }
            }
        }
        if let Some(upload) = self
            .arenas
            .rounded
            .upload(&self.device, &self.queue, &border_verts)
        {
            render_pass.set_pipeline(&self.pipelines.rounded_rect);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, upload.buffer_slice());
            render_pass.draw(0..border_verts.len() as u32, 0..1);
        }
    }

    /// Extended decorative effect stack: window pattern overlays interleaved
    /// with decorative cursor art, in fixed z-order.
    fn draw_pre_content_effects_extended(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        ctx: &super::effect_common::EffectCtx<'_>,
    ) {
        // Aurora/northern lights effect
        draw_stateful!(
            self,
            render_pass,
            "Aurora",
            super::window_effects::emit_aurora_overlay(ctx)
        );

        // === Heat distortion effect ===
        draw_effect!(
            self,
            render_pass,
            "Heat Distortion Buffer",
            super::pattern_effects::emit_heat_distortion(ctx),
            continuous
        );

        // === Cursor lighthouse beam ===
        draw_effect!(
            self,
            render_pass,
            "Lighthouse Beam Buffer",
            super::cursor_effects::emit_cursor_lighthouse_beam(ctx),
            continuous
        );

        // === Neon border effect ===
        draw_effect!(
            self,
            render_pass,
            "Neon Border Buffer",
            super::pattern_effects::emit_neon_border(ctx),
            continuous
        );

        // === Cursor sonar ping effect ===
        draw_stateful!(
            self,
            render_pass,
            "Sonar Ping Buffer",
            super::cursor_effects::emit_cursor_sonar_ping(ctx, &mut self.fx.sonar_ping.entries)
        );

        // === Lightning bolt effect ===
        draw_stateful!(
            self,
            render_pass,
            "Lightning Bolt Buffer",
            super::cursor_effects::emit_lightning_bolt(
                ctx,
                &mut self.clocks.lightning_bolt_last,
                &mut self.fx.lightning_bolt.segments,
                &mut self.fx.lightning_bolt.age
            )
        );

        // === Cursor orbit particles effect ===
        draw_effect!(
            self,
            render_pass,
            "Orbit Particles Buffer",
            super::cursor_effects::emit_cursor_orbit_particles(ctx),
            continuous
        );

        // === Plasma border effect ===
        draw_effect!(
            self,
            render_pass,
            "Plasma Border Buffer",
            super::pattern_effects::emit_plasma_border(ctx),
            continuous
        );

        // === Cursor heartbeat pulse effect ===
        draw_effect!(
            self,
            render_pass,
            "Heartbeat Pulse Buffer",
            super::cursor_effects::emit_cursor_heartbeat_pulse(ctx),
            continuous
        );

        // === Topographic contour effect ===
        draw_effect!(
            self,
            render_pass,
            "Topo Contour Buffer",
            super::pattern_effects::emit_topographic_contour(ctx),
            continuous
        );

        // === Cursor metronome tick effect ===
        draw_stateful!(
            self,
            render_pass,
            "Metronome Tick Buffer",
            super::cursor_effects::emit_cursor_metronome_tick(
                ctx,
                &mut self.fx.metronome.last_x,
                &mut self.fx.metronome.last_y,
                &mut self.fx.metronome.tick_start
            )
        );

        // === Constellation overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "Constellation Buffer",
            super::pattern_effects::emit_constellation(ctx),
            continuous
        );

        // === Cursor radar sweep effect ===
        draw_effect!(
            self,
            render_pass,
            "Radar Sweep Buffer",
            super::cursor_effects::emit_cursor_radar_sweep(ctx),
            continuous
        );

        // === Kaleidoscope overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "Kaleidoscope Buffer",
            super::pattern_effects::emit_kaleidoscope(ctx),
            continuous
        );

        // === Cursor ripple ring effect ===
        draw_stateful!(
            self,
            render_pass,
            "Ripple Ring Buffer",
            super::cursor_effects::emit_cursor_ripple_ring(
                ctx,
                &mut self.fx.ripple_ring.start,
                &mut self.fx.ripple_ring.last_x,
                &mut self.fx.ripple_ring.last_y
            )
        );

        // === Noise field overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "Noise Field Buffer",
            super::pattern_effects::emit_noise_field(ctx),
            continuous
        );

        // === Cursor scope effect ===
        draw_effect!(
            self,
            render_pass,
            "Cursor Scope Buffer",
            super::cursor_effects::emit_cursor_scope(ctx),
            continuous
        );

        // === Spiral vortex overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "Spiral Vortex Buffer",
            super::pattern_effects::emit_spiral_vortex(ctx),
            continuous
        );

        // === Cursor shockwave effect ===
        draw_stateful!(
            self,
            render_pass,
            "Shockwave Buffer",
            super::cursor_effects::emit_cursor_shockwave(
                ctx,
                &mut self.fx.shockwave.start,
                &mut self.fx.shockwave.last_x,
                &mut self.fx.shockwave.last_y
            )
        );

        // === Diamond lattice overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "Diamond Lattice Buffer",
            super::pattern_effects::emit_diamond_lattice(ctx),
            continuous
        );

        // === Cursor gravity well effect ===
        draw_effect!(
            self,
            render_pass,
            "Gravity Well Buffer",
            super::cursor_effects::emit_cursor_gravity_well(ctx),
            continuous
        );

        // === Wave interference overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "Wave Interference Buffer",
            super::pattern_effects::emit_wave_interference(ctx),
            continuous
        );

        // === Cursor portal effect ===
        draw_effect!(
            self,
            render_pass,
            "Cursor Portal Buffer",
            super::cursor_effects::emit_cursor_portal(ctx),
            continuous
        );

        // === Chevron pattern overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "Chevron Pattern Buffer",
            super::pattern_effects::emit_chevron(ctx),
            continuous
        );

        // === Cursor bubble effect ===
        draw_stateful!(
            self,
            render_pass,
            "Cursor Bubble Buffer",
            super::cursor_effects::emit_cursor_bubble(
                ctx,
                &mut self.fx.bubble.spawn_time,
                &mut self.fx.bubble.last_x,
                &mut self.fx.bubble.last_y
            )
        );

        // === Sunburst pattern overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "sunburst_pattern_vb",
            super::pattern_effects::emit_sunburst(ctx),
            continuous
        );

        // === Cursor firework effect ===
        draw_stateful!(
            self,
            render_pass,
            "cursor_firework_vb",
            super::cursor_effects::emit_cursor_firework(
                ctx,
                &mut self.fx.firework.start,
                &mut self.fx.firework.last_x,
                &mut self.fx.firework.last_y
            )
        );

        // === Honeycomb dissolve overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "honeycomb_dissolve_vb",
            super::pattern_effects::emit_honeycomb_dissolve(ctx),
            continuous
        );

        // === Cursor tornado effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_tornado_vb",
            super::cursor_effects::emit_cursor_tornado(ctx),
            continuous
        );

        // === Moiré pattern overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "moire_pattern_vb",
            super::pattern_effects::emit_moire(ctx),
            continuous
        );

        // === Cursor lightning effect ===
        draw_stateful!(
            self,
            render_pass,
            "cursor_lightning_vb",
            super::cursor_effects::emit_cursor_lightning(
                ctx,
                &mut self.fx.cursor_lightning.start,
                &mut self.fx.cursor_lightning.last_x,
                &mut self.fx.cursor_lightning.last_y
            )
        );

        // === Dot matrix overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "dot_matrix_vb",
            super::pattern_effects::emit_dot_matrix(ctx),
            continuous
        );

        // === Cursor snowflake effect ===
        draw_stateful!(
            self,
            render_pass,
            "cursor_snowflake_vb",
            super::cursor_effects::emit_cursor_snowflake(
                ctx,
                &mut self.fx.snowflake.start,
                &mut self.fx.snowflake.last_x,
                &mut self.fx.snowflake.last_y
            )
        );

        // === Concentric rings overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "concentric_rings_vb",
            super::pattern_effects::emit_concentric_rings(ctx),
            continuous
        );

        // === Cursor flame effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_flame_vb",
            super::cursor_effects::emit_cursor_flame(ctx),
            continuous
        );

        // === Zigzag pattern overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "zigzag_pattern_vb",
            super::pattern_effects::emit_zigzag(ctx),
            continuous
        );

        // === Cursor crystal effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_crystal_vb",
            super::cursor_effects::emit_cursor_crystal(ctx),
            continuous
        );

        // === Tessellation overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "tessellation_verts",
            super::pattern_effects::emit_tessellation(ctx)
        );

        // === Cursor water drop effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_water_drop_verts",
            super::cursor_effects::emit_cursor_water_drop(ctx),
            continuous
        );

        // === Guilloche overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "guilloche_verts",
            super::pattern_effects::emit_guilloche(ctx),
            continuous
        );

        // === Cursor pixel dust effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_pixel_dust_verts",
            super::cursor_effects::emit_cursor_pixel_dust(ctx),
            continuous
        );

        // === Celtic knot overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "celtic_knot_verts",
            super::pattern_effects::emit_celtic_knot(ctx),
            continuous
        );

        // === Cursor candle flame effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_candle_flame_verts",
            super::cursor_effects::emit_cursor_candle_flame(ctx),
            continuous
        );

        // === Argyle pattern overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "argyle_pattern_verts",
            super::pattern_effects::emit_argyle(ctx)
        );

        // === Cursor moth flame effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_moth_flame_verts",
            super::cursor_effects::emit_cursor_moth_flame(ctx),
            continuous
        );

        // === Basket weave overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "basket_weave_verts",
            super::pattern_effects::emit_basket_weave(ctx)
        );

        // === Cursor sparkler effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_sparkler_verts",
            super::cursor_effects::emit_cursor_sparkler(ctx),
            continuous
        );

        // === Fish scale overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "fish_scale_verts",
            super::pattern_effects::emit_fish_scale(ctx)
        );

        // === Cursor plasma ball effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_plasma_ball_verts",
            super::cursor_effects::emit_cursor_plasma_ball(ctx),
            continuous
        );

        // === Trefoil knot overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "trefoil_knot_verts",
            super::pattern_effects::emit_trefoil_knot(ctx),
            continuous
        );

        // === Cursor quill pen effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_quill_pen_verts",
            super::cursor_effects::emit_cursor_quill_pen(ctx),
            continuous
        );

        // === Herringbone pattern overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "herringbone_pattern_verts",
            super::pattern_effects::emit_herringbone(ctx)
        );

        // === Cursor aurora borealis effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_aurora_borealis_verts",
            super::cursor_effects::emit_cursor_aurora_borealis(ctx),
            continuous
        );

        // === Target reticle overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "target_reticle_verts",
            super::pattern_effects::emit_target_reticle(ctx),
            continuous
        );

        // === Cursor feather effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_feather_verts",
            super::cursor_effects::emit_cursor_feather(ctx),
            continuous
        );

        // === Plaid pattern overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "plaid_pattern_verts",
            super::pattern_effects::emit_plaid(ctx)
        );

        // === Cursor stardust effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_stardust_verts",
            super::cursor_effects::emit_cursor_stardust(ctx),
            continuous
        );

        // === Brick wall overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "brick_wall_verts",
            super::pattern_effects::emit_brick_wall(ctx)
        );

        // === Cursor compass needle effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_compass_needle_verts",
            super::cursor_effects::emit_cursor_compass_needle(ctx),
            continuous
        );

        // === Sine wave overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "sine_wave_verts",
            super::pattern_effects::emit_sine_wave(ctx),
            continuous
        );

        // === Cursor galaxy effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_galaxy_verts",
            super::cursor_effects::emit_cursor_galaxy(ctx),
            continuous
        );

        // === Rotating gear overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "rotating_gear_verts",
            super::pattern_effects::emit_rotating_gear(ctx),
            continuous
        );

        // === Cursor prism effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_prism_verts",
            super::cursor_effects::emit_cursor_prism(ctx),
            continuous
        );

        // === Crosshatch pattern overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "crosshatch_pattern_verts",
            super::pattern_effects::emit_crosshatch(ctx),
            continuous
        );

        // === Cursor moth effect ===
        draw_effect!(
            self,
            render_pass,
            "cursor_moth_verts",
            super::cursor_effects::emit_cursor_moth(ctx),
            continuous
        );

        // === Hex grid overlay effect ===
        draw_effect!(
            self,
            render_pass,
            "Hex Grid Buffer",
            super::pattern_effects::emit_hex_grid(ctx),
            continuous
        );

        // === Cursor sparkle burst effect ===
        draw_stateful!(
            self,
            render_pass,
            "Sparkle Burst Buffer",
            super::cursor_effects::emit_cursor_sparkle_burst(
                ctx,
                &mut self.fx.sparkle_burst.entries
            )
        );

        // === Circuit board trace effect ===
        draw_effect!(
            self,
            render_pass,
            "Circuit Trace Buffer",
            super::pattern_effects::emit_circuit_board(ctx),
            continuous
        );

        // === Cursor compass rose effect ===
        draw_effect!(
            self,
            render_pass,
            "Compass Rose Buffer",
            super::cursor_effects::emit_cursor_compass_rose(ctx),
            continuous
        );

        // === Warp/distortion grid effect ===
        draw_effect!(
            self,
            render_pass,
            "Warp Grid Buffer",
            super::pattern_effects::emit_warp_grid(ctx),
            continuous
        );

        // === Cursor DNA helix trail effect ===
        draw_effect!(
            self,
            render_pass,
            "DNA Helix Buffer",
            super::cursor_effects::emit_cursor_dna_helix(ctx),
            continuous
        );

        // === Prism/rainbow edge effect ===
        draw_effect!(
            self,
            render_pass,
            "Prism Edge Buffer",
            super::pattern_effects::emit_prism_rainbow_edge(ctx),
            continuous
        );

        // === Cursor pendulum swing effect ===
        draw_stateful!(
            self,
            render_pass,
            "Pendulum Buffer",
            super::cursor_effects::emit_cursor_pendulum(
                ctx,
                &mut self.fx.pendulum.last_x,
                &mut self.fx.pendulum.last_y,
                &mut self.fx.pendulum.swing_start
            )
        );

        // === Cursor drop shadow (drawn before cursor bg) ===
        draw_effect!(
            self,
            render_pass,
            "Cursor Shadow Buffer",
            super::cursor_effects::emit_cursor_drop_shadow(ctx)
        );
    }

    /// Draw the post-content effect stacks on top of text and media.
    pub(super) fn draw_post_content_effects(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        ctx: &super::effect_common::EffectCtx<'_>,
        faces: &HashMap<FaceId, Face>,
    ) {
        self.draw_post_window_chrome_effects(render_pass, ctx);
        self.draw_post_focus_and_dimming_effects(render_pass, ctx);
        self.draw_post_highlight_effects(render_pass, ctx, faces);
        self.draw_post_navigation_effects(render_pass, ctx);
        self.draw_post_resize_and_input_effects(render_pass, ctx);
        self.draw_post_overlay_effects(render_pass, ctx);
    }

    fn draw_post_window_chrome_effects(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        ctx: &super::effect_common::EffectCtx<'_>,
    ) {
        // === Draw mode-line separators ===
        draw_effect!(
            self,
            render_pass,
            "Mode-line Separator Buffer",
            super::window_effects::emit_mode_line_separator(ctx)
        );

        // === Buffer-local accent color strip ===
        draw_effect!(
            self,
            render_pass,
            "Accent Strip Buffer",
            super::window_effects::emit_accent_strip(ctx)
        );

        // === Window background tint based on file type ===
        draw_effect!(
            self,
            render_pass,
            "Mode Tint Buffer",
            super::window_effects::emit_window_mode_tint(ctx)
        );

        // === Animated focus ring (marching ants) around selected window ===
        draw_stateful!(
            self,
            render_pass,
            "Focus Ring Buffer",
            super::window_effects::emit_focus_ring(ctx, self.clocks.focus_ring_start)
        );

        // === Window padding gradient (inner edge shading for depth) ===
        draw_effect!(
            self,
            render_pass,
            "Padding Gradient Buffer",
            super::window_effects::emit_window_padding_gradient(ctx)
        );

        // === Smooth border color transition on focus ===
        draw_stateful!(
            self,
            render_pass,
            "Border Transition Buffer",
            super::window_effects::emit_border_transition(
                ctx,
                &mut self.fx.border_transition.transitions,
                &mut self.fx.border_transition.prev_selected,
                self.durations.border_transition,
            )
        );
    }

    fn draw_post_focus_and_dimming_effects(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        ctx: &super::effect_common::EffectCtx<'_>,
    ) {
        // === Frosted glass effect on mode-lines ===
        draw_effect!(
            self,
            render_pass,
            "Frosted Glass Buffer",
            super::window_effects::emit_frosted_glass(ctx)
        );

        // === Noise/film grain texture overlay ===
        draw_stateful!(
            self,
            render_pass,
            "Noise Grain Buffer",
            super::window_effects::emit_noise_grain(ctx, &mut self.fx.noise_grain.frame)
        );

        // === Idle screen dimming ===
        draw_effect!(
            self,
            render_pass,
            "Idle Dim Buffer",
            super::window_effects::emit_idle_dimming(ctx, self.fx.dim.idle_alpha)
        );

        // === Focus mode: dim lines outside current paragraph ===
        draw_effect!(
            self,
            render_pass,
            "Focus Mode Buffer",
            super::window_effects::emit_focus_mode(ctx)
        );

        // === Draw inactive window dimming overlays (with smooth fade) ===
        draw_stateful!(
            self,
            render_pass,
            "Inactive Dim Buffer",
            super::window_effects::emit_inactive_window_dimming(
                ctx,
                &mut self.fx.dim.per_window,
                &mut self.clocks.last_dim_tick,
            )
        );

        // === Inactive window color tint ===
        draw_effect!(
            self,
            render_pass,
            "Inactive Tint Buffer",
            super::window_effects::emit_inactive_window_tint(ctx)
        );
    }

    fn draw_post_highlight_effects(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        ctx: &super::effect_common::EffectCtx<'_>,
        faces: &HashMap<FaceId, Face>,
    ) {
        // === Zen mode: draw margin overlays for centered content ===
        draw_effect!(
            self,
            render_pass,
            "Zen Mode Buffer",
            super::window_effects::emit_zen_mode(ctx)
        );

        // === Cursor trail fade (afterimage ghost) ===
        draw_stateful!(
            self,
            render_pass,
            "Cursor Trail Buffer",
            super::cursor_effects::emit_cursor_trail_fade(
                ctx,
                &mut self.fx.cursor_trail.positions,
                &self.durations.cursor_trail_fade,
            )
        );

        // === Search highlight pulse (glow on isearch face glyphs) ===
        draw_stateful!(
            self,
            render_pass,
            "Search Pulse Buffer",
            super::window_effects::emit_search_highlight(ctx, self.clocks.search_pulse_start)
        );

        // === Selection region glow highlight ===
        draw_effect!(
            self,
            render_pass,
            "Region Glow Buffer",
            super::window_effects::emit_selection_glow(ctx, faces)
        );

        // === Typing ripple effect ===
        draw_stateful!(
            self,
            render_pass,
            "Ripple Buffer",
            super::window_effects::emit_typing_ripple(
                ctx,
                &mut self.fx.typing_ripple.active,
                self.durations.typing_ripple,
            )
        );
    }

    fn draw_post_navigation_effects(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        ctx: &super::effect_common::EffectCtx<'_>,
    ) {
        // === Minimap: code overview column on right side of each window ===
        draw_effect!(
            self,
            render_pass,
            "Minimap Buffer",
            super::window_effects::emit_minimap(ctx)
        );

        // === Header/mode-line shadow depth effect ===
        draw_effect!(
            self,
            render_pass,
            "Header Shadow Buffer",
            super::window_effects::emit_header_shadow(ctx)
        );

        // === Active window border glow ===
        draw_effect!(
            self,
            render_pass,
            "Window Glow Buffer",
            super::window_effects::emit_active_window_glow(ctx)
        );

        // === Scroll progress indicator bar ===
        draw_effect!(
            self,
            render_pass,
            "Scroll Progress Buffer",
            super::window_effects::emit_scroll_progress(ctx)
        );

        // === Window content shadow/depth effect ===
        draw_effect!(
            self,
            render_pass,
            "Window Content Shadow Buffer",
            super::window_effects::emit_window_content_shadow(ctx)
        );
    }

    fn draw_post_resize_and_input_effects(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        ctx: &super::effect_common::EffectCtx<'_>,
    ) {
        // === Resize padding transition overlay ===
        {
            let pad = self.resize_padding_amount();
            draw_stateful!(
                self,
                render_pass,
                "Resize Padding Buffer",
                super::window_effects::emit_resize_padding(ctx, pad)
            );
            if pad <= 0.5 && self.fx.resize_padding.started.is_some() {
                // Animation complete, clean up
                self.fx.resize_padding.started = None;
            }
        }

        // === Mini-buffer completion highlight ===
        draw_effect!(
            self,
            render_pass,
            "Minibuffer Highlight Buffer",
            super::window_effects::emit_minibuffer_completion(ctx)
        );

        // === Scroll velocity fade overlay ===
        draw_stateful!(
            self,
            render_pass,
            "Scroll Velocity Fade Buffer",
            super::window_effects::emit_scroll_velocity_fade(
                ctx,
                &mut self.fx.scroll_velocity.fades,
            )
        );

        // === Click halo effect ===
        draw_stateful!(
            self,
            render_pass,
            "Click Halo Buffer",
            super::window_effects::emit_click_halo(ctx, &mut self.fx.click_halo.halos,)
        );

        // === Window edge snap indicator ===
        draw_stateful!(
            self,
            render_pass,
            "Edge Snap Buffer",
            super::window_effects::emit_edge_snap(ctx, &mut self.fx.edge_snap.snaps,)
        );
    }

    fn draw_post_overlay_effects(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        ctx: &super::effect_common::EffectCtx<'_>,
    ) {
        // === Line wrap indicator overlay ===
        draw_effect!(
            self,
            render_pass,
            "Wrap Indicator Buffer",
            super::window_effects::emit_line_wrap_indicator(ctx)
        );

        // === Scroll momentum indicator ===
        draw_stateful!(
            self,
            render_pass,
            "Scroll Momentum Buffer",
            super::window_effects::emit_scroll_momentum(ctx, &self.fx.scroll_momentum.active,)
        );

        // === Vignette effect: darken edges of the frame ===
        draw_effect!(
            self,
            render_pass,
            "Vignette Buffer",
            super::window_effects::emit_vignette(ctx)
        );

        // === Window switch highlight fade ===
        draw_stateful!(
            self,
            render_pass,
            "Window Switch Fade Buffer",
            super::window_effects::emit_window_switch_fade(ctx, &mut self.fx.window_fade.active,)
        );
    }
}
