//! Cursor visual effect methods.
//!
//! Contains all cursor-related visual effects extracted from the main
//! render_frame_glyphs function. Each effect computes vertices that are
//! then drawn by the caller in the render pass.

use super::super::vertex::RectVertex;
use super::effect_common::effect_entity_seed;
use super::effect_common::{EffectCtx, ambient_cycle_phase, find_cursor_pos, push_rect};
use super::{CursorParticle, MatrixColumn, RippleWaveEntry, SonarPingEntry, SparkleBurstEntry};
use neomacs_display_protocol::frame_time::EventTime;
use neomacs_display_protocol::types::Color;

/// Emit cursor glow effect vertices.
pub(super) fn emit_cursor_glow(ctx: &EffectCtx, cursor_pulse_start: &EventTime) -> Vec<RectVertex> {
    if !ctx.effects.cursor_glow.enabled || !ctx.cursor_visible {
        return Vec::new();
    }

    let glow_pos = find_cursor_pos(ctx.animated_cursor, ctx.frame_glyphs);

    if let Some((cx, cy, cw, ch)) = glow_pos {
        let (gr, gg, gb) = ctx.effects.cursor_glow.color;
        let radius = ctx.effects.cursor_glow.radius;
        let mut peak_alpha = ctx.effects.cursor_glow.opacity;

        // Apply pulse modulation if enabled
        if ctx.effects.cursor_pulse.enabled {
            let elapsed = ctx
                .frame_sample
                .since_at_presentation(*cursor_pulse_start)
                .as_secs_f32();
            let phase = elapsed * ctx.effects.cursor_pulse.speed * 2.0 * std::f32::consts::PI;
            let t = (phase.sin() + 1.0) / 2.0;
            let factor = ctx.effects.cursor_pulse.min_opacity
                + t * (1.0 - ctx.effects.cursor_pulse.min_opacity);
            peak_alpha *= factor;
        }
        let layers = (radius / 2.0).ceil() as i32;
        let mut verts: Vec<RectVertex> = Vec::new();

        let center_x = cx + cw / 2.0;
        let center_y = cy + ch / 2.0;

        for i in 0..layers {
            let t = (i + 1) as f32 / layers as f32;
            let r = radius * t;
            let alpha = peak_alpha * (1.0 - t * t);
            let c = Color::new(gr, gg, gb, alpha);
            push_rect(&mut verts, center_x - r, center_y - r, r * 2.0, r * 2.0, &c);
        }

        verts
    } else {
        Vec::new()
    }
}

/// Emit cursor crosshair guide line vertices.
///
/// Draws a full-width horizontal line and a full-height vertical line
/// through the active cursor position, constrained to the selected window.
pub(super) fn emit_cursor_crosshair(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_crosshair.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let cross_pos = if let Some(anim) = ctx.animated_cursor {
        Some((anim.x, anim.y, anim.width, anim.height))
    } else {
        ctx.frame_glyphs
            .active_cursor()
            .map(|cursor| (cursor.x, cursor.y, cursor.width, cursor.height))
    };
    let mut verts = Vec::new();
    if let Some((cx, cy, cw, ch)) = cross_pos {
        let cursor_center_x = cx + cw / 2.0;
        let cursor_center_y = cy + ch / 2.0;
        if let Some(win_info) = ctx
            .frame_glyphs
            .window_infos
            .iter()
            .find(|w| w.selected && !w.is_minibuffer)
        {
            let (cr, cg, cb) = ctx.effects.cursor_crosshair.color;
            let alpha = ctx.effects.cursor_crosshair.opacity;
            let c = Color::new(cr, cg, cb, alpha);
            let wb = &win_info.bounds;
            // Don't extend into mode-line area
            let win_bottom = wb.y + wb.height - win_info.mode_line_height;
            // Horizontal line (full window width, 1px height at cursor center Y)
            push_rect(&mut verts, wb.x, cursor_center_y, wb.width, 1.0, &c);
            // Vertical line (1px width, from window top to above mode-line)
            push_rect(
                &mut verts,
                cursor_center_x,
                wb.y,
                1.0,
                win_bottom - wb.y,
                &c,
            );
        }
    }
    verts
}

/// Emit cursor magnetism collapsing ring vertices.
///
/// Detects large cursor jumps and renders concentric rings that collapse
/// inward at the jump origin, fading over time.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_magnetism(
    ctx: &EffectCtx,
    entries: &mut Vec<(f32, f32, EventTime)>,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.cursor_magnetism.enabled {
        return (Vec::new(), false);
    }
    let now = ctx.frame_sample.presentation_time();
    let dur = std::time::Duration::from_millis(ctx.effects.cursor_magnetism.duration_ms as u64);

    // Detect cursor jump (large movement) and record
    if let Some(anim) = ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        let should_add = entries
            .last()
            .map(|(px, py, _)| {
                let dx = (cx - px).abs();
                let dy = (cy - py).abs();
                dx > 50.0 || dy > 20.0 // jump threshold
            })
            .unwrap_or(false);
        if should_add {
            entries.push((cx, cy, now));
        }
        // Initial entry
        if entries.is_empty() {
            entries.push((cx, cy, now));
        }
    }

    // Prune expired
    entries.retain(|&(_, _, t)| now.saturating_since(t) < dur);

    // Render collapsing rings
    let mut needs_redraw = false;
    let mut verts = Vec::new();
    if !entries.is_empty() {
        let (mr, mg, mb) = ctx.effects.cursor_magnetism.color;
        let max_alpha = ctx.effects.cursor_magnetism.opacity;
        let ring_count = ctx.effects.cursor_magnetism.ring_count;
        for &(cx, cy, started) in entries.iter() {
            let t = now.saturating_since(started).as_secs_f32() / dur.as_secs_f32();
            if t >= 1.0 {
                continue;
            }
            let alpha = max_alpha * (1.0 - t);
            for ring in 0..ring_count {
                let ring_t = ring as f32 / ring_count as f32;
                let radius = (1.0 - t) * (30.0 + ring_t * 40.0); // collapse inward
                let c = Color::new(mr, mg, mb, alpha * (1.0 - ring_t * 0.5));
                let line_w = 1.5;
                // Approximate ring with 4 rects (top, bottom, left, right)
                push_rect(
                    &mut verts,
                    cx - radius,
                    cy - radius,
                    radius * 2.0,
                    line_w,
                    &c,
                );
                push_rect(
                    &mut verts,
                    cx - radius,
                    cy + radius - line_w,
                    radius * 2.0,
                    line_w,
                    &c,
                );
                push_rect(
                    &mut verts,
                    cx - radius,
                    cy - radius,
                    line_w,
                    radius * 2.0,
                    &c,
                );
                push_rect(
                    &mut verts,
                    cx + radius - line_w,
                    cy - radius,
                    line_w,
                    radius * 2.0,
                    &c,
                );
            }
        }
        needs_redraw = true;
    }
    (verts, needs_redraw)
}

/// Emit line number pulse overlay on the cursor line.
///
/// Renders a pulsing highlight over the line-number gutter area of the
/// cursor's current row, using a sinusoidal brightness cycle.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_line_number_pulse(ctx: &EffectCtx) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.line_number_pulse.enabled {
        return (Vec::new(), false);
    }
    let mut verts = Vec::new();
    let mut needs_redraw = false;
    if let Some(anim) = ctx.animated_cursor {
        // Free-running: the gutter pulse has no trigger event, so its phase is
        // the session-start anchor aged against this frame's sample, one full
        // cycle every `cycle_ms` (2s by default).
        let phase = ambient_cycle_phase(ctx, ctx.effects.line_number_pulse.cycle_ms);
        let pulse = ((phase * std::f64::consts::TAU).sin() * 0.5 + 0.5) as f32;
        let alpha = ctx.effects.line_number_pulse.intensity * pulse;
        let (pr, pg, pb) = ctx.effects.line_number_pulse.color;
        if alpha > 0.001 {
            for win_info in &ctx.frame_glyphs.window_infos {
                if !win_info.selected {
                    continue;
                }
                let b = &win_info.bounds;
                let gutter_width = 40.0_f32;
                let c = Color::new(pr, pg, pb, alpha);
                push_rect(&mut verts, b.x, anim.y, gutter_width, anim.height, &c);
            }
        }
        needs_redraw = true;
    }
    (verts, needs_redraw)
}

/// Emit cursor spotlight / radial gradient overlay vertices.
///
/// Draws concentric square layers around the active cursor position with
/// quadratic alpha falloff to approximate a radial gradient spotlight.
pub(super) fn emit_cursor_spotlight(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_spotlight.enabled {
        return Vec::new();
    }
    let mut verts = Vec::new();
    if let Some(anim) = ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        let radius = ctx.effects.cursor_spotlight.radius;
        let intensity = ctx.effects.cursor_spotlight.intensity;
        let (spr, spg, spb) = ctx.effects.cursor_spotlight.color;
        // Approximate radial gradient with concentric rings
        let rings = 20u32;
        for i in (0..rings).rev() {
            let t = (i as f32 + 1.0) / rings as f32;
            let r = radius * t;
            let alpha = intensity * (1.0 - t) * (1.0 - t); // quadratic falloff
            if alpha > 0.001 {
                let c = Color::new(spr, spg, spb, alpha);
                push_rect(&mut verts, cx - r, cy - r, r * 2.0, r * 2.0, &c);
            }
        }
    }
    verts
}

/// Emit cursor comet tail vertices.
///
/// Records past cursor positions and renders fading ghost cursors along
/// the trail, creating a comet tail effect behind the moving cursor.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_comet(
    ctx: &EffectCtx,
    positions: &mut Vec<(f32, f32, f32, f32, EventTime)>,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.cursor_comet.enabled {
        return (Vec::new(), false);
    }
    let now = ctx.frame_sample.presentation_time();
    let fade_dur = std::time::Duration::from_millis(ctx.effects.cursor_comet.fade_ms as u64);

    // Record cursor position
    if let Some(anim) = ctx.animated_cursor {
        // Only record if position changed
        let should_add = positions
            .last()
            .map(|(px, py, _, _, _)| (anim.x - px).abs() > 0.5 || (anim.y - py).abs() > 0.5)
            .unwrap_or(true);
        if should_add {
            positions.push((anim.x, anim.y, anim.width, anim.height, now));
        }
    }

    // Prune old positions
    positions.retain(|&(_, _, _, _, t)| now.saturating_since(t) < fade_dur);

    // Keep only trail_length most recent
    let max = ctx.effects.cursor_comet.trail_length as usize;
    if positions.len() > max {
        let drain_count = positions.len() - max;
        positions.drain(0..drain_count);
    }

    // Render ghost cursors
    let mut needs_redraw = false;
    let mut verts = Vec::new();
    if !positions.is_empty() {
        let (cr, cg, cb) = ctx.effects.cursor_comet.color;
        let max_alpha = ctx.effects.cursor_comet.opacity;
        let count = positions.len();
        for (i, &(px, py, pw, ph, t)) in positions.iter().enumerate() {
            let age = now.saturating_since(t).as_secs_f32();
            let time_fade = 1.0 - (age / fade_dur.as_secs_f32()).min(1.0);
            let pos_fade = (i as f32 + 1.0) / count as f32;
            let alpha = max_alpha * time_fade * pos_fade * 0.5;
            if alpha > 0.001 {
                let c = Color::new(cr, cg, cb, alpha);
                push_rect(&mut verts, px, py, pw, ph, &c);
            }
        }
        needs_redraw = true;
    }
    (verts, needs_redraw)
}

/// Emit cursor particle trail vertices.
///
/// When the cursor moves, particles are emitted from the cursor center
/// with random velocities and gravity, shrinking and fading over their lifetime.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_particles(
    ctx: &EffectCtx,
    particles: &mut Vec<CursorParticle>,
    prev_pos: &mut Option<(f32, f32)>,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.cursor_particles.enabled {
        return (Vec::new(), false);
    }
    let now = ctx.frame_sample.presentation_time();
    let lifetime =
        std::time::Duration::from_millis(ctx.effects.cursor_particles.lifetime_ms as u64);

    // Detect cursor movement and emit particles
    if let Some(anim) = ctx.animated_cursor {
        let cur_pos = (anim.x + anim.width / 2.0, anim.y + anim.height / 2.0);
        if let Some(prev) = *prev_pos {
            let dx = (cur_pos.0 - prev.0).abs();
            let dy = (cur_pos.1 - prev.1).abs();
            if dx > 1.0 || dy > 1.0 {
                // Emit particles from cursor center
                // RNG SEED, NOT A TIME: this read `now.elapsed()` -- the few
                // hundred nanoseconds since `now` was minted -- purely for its
                // low bits. The frame's time sample is constant across a frame,
                // so substituting it would give every particle in the burst the
                // same trajectory. Seed from the frame counter and the
                // particle's index instead: distinct within a frame, varying
                // across frames, and reproducible for a replayed frame.
                let spawn_base = particles.len() as u64;
                for i in 0..ctx.effects.cursor_particles.count {
                    let h = effect_entity_seed(ctx.frame_seq, spawn_base.wrapping_add(i as u64));
                    let rx = ((h >> 16) & 0xFFFF) as f32 / 65535.0 - 0.5; // -0.5..0.5
                    let ry = ((h >> 32) & 0xFFFF) as f32 / 65535.0 - 0.5;
                    particles.push(CursorParticle {
                        x: cur_pos.0,
                        y: cur_pos.1,
                        vx: rx * 80.0,        // random horizontal velocity
                        vy: ry * 60.0 - 30.0, // slight upward bias
                        started: now,
                        lifetime,
                    });
                }
            }
        }
        *prev_pos = Some(cur_pos);
    }

    // Prune expired particles
    particles.retain(|p| now.saturating_since(p.started) < p.lifetime);

    // Render particles
    let mut needs_redraw = false;
    let mut verts = Vec::new();
    if !particles.is_empty() {
        let (pr, pg, pb) = ctx.effects.cursor_particles.color;
        let gravity = ctx.effects.cursor_particles.gravity;
        for p in particles.iter() {
            let elapsed = now.saturating_since(p.started).as_secs_f32();
            let t = (elapsed / p.lifetime.as_secs_f32()).min(1.0);
            let alpha = (1.0 - t) * 0.8;
            if alpha > 0.001 {
                let px = p.x + p.vx * elapsed;
                let py = p.y + p.vy * elapsed + 0.5 * gravity * elapsed * elapsed;
                let size = 2.0 * (1.0 - t) + 0.5; // shrink over time
                let c = Color::new(pr, pg, pb, alpha);
                push_rect(&mut verts, px - size / 2.0, py - size / 2.0, size, size, &c);
            }
        }
        // Keep redrawing while particles exist
        needs_redraw = true;
    }
    (verts, needs_redraw)
}

/// Emit matrix / digital rain effect vertices.
///
/// Renders columns of falling vertical gradient strips that wrap around
/// the screen, simulating a digital rain / Matrix effect.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_matrix_rain(
    ctx: &EffectCtx,
    columns: &mut Vec<MatrixColumn>,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.matrix_rain.enabled {
        return (Vec::new(), false);
    }
    let fw = ctx.surface_width as f32 / ctx.scale_factor;
    let fh = ctx.surface_height as f32 / ctx.scale_factor;
    let dt = 1.0 / 60.0_f32;
    // RNG SEED, NOT A TIME: the column layout was seeded from
    // `Instant::now().elapsed().subsec_nanos()`, entropy dressed up as a clock
    // read. The frame's time sample is constant across a frame, so it would
    // collapse every column to one track. Each column is seeded from the frame
    // counter and its own index instead.

    // Spawn columns if needed
    while columns.len() < ctx.effects.matrix_rain.column_count as usize {
        let i = columns.len() as u64;
        let h = effect_entity_seed(ctx.frame_seq, i);
        let x = (i as f32 / ctx.effects.matrix_rain.column_count as f32) * fw;
        let y = -(((h >> 16) & 0xFFFF) as f32 / 65535.0) * fh;
        let speed_var = 0.6 + ((h >> 32) & 0xFFFF) as f32 / 65535.0 * 0.8;
        let length = 30.0 + ((h >> 48) & 0xFF) as f32 / 255.0 * 80.0;
        columns.push(MatrixColumn {
            x,
            y,
            speed: ctx.effects.matrix_rain.speed * speed_var,
            length,
        });
    }

    // Update positions
    for col in columns.iter_mut() {
        col.y += col.speed * dt;
        if col.y - col.length > fh {
            // A recycled column keeps its x as its identity, so it draws a
            // fresh track every time it wraps rather than repeating one.
            let h = effect_entity_seed(ctx.frame_seq, (col.x * 1000.0) as u64);
            col.y = -(((h >> 16) & 0xFFFF) as f32 / 65535.0) * 50.0;
            let speed_var = 0.6 + ((h >> 32) & 0xFFFF) as f32 / 65535.0 * 0.8;
            col.speed = ctx.effects.matrix_rain.speed * speed_var;
            col.length = 30.0 + ((h >> 48) & 0xFF) as f32 / 255.0 * 80.0;
        }
    }

    // Render columns as vertical gradient strips
    let (mr, mg, mb) = ctx.effects.matrix_rain.color;
    let mut verts: Vec<RectVertex> = Vec::new();
    for col in columns.iter() {
        let segments = 10u32;
        let seg_h = col.length / segments as f32;
        for s in 0..segments {
            let y = col.y - col.length + s as f32 * seg_h;
            if y + seg_h < 0.0 || y > fh {
                continue;
            }
            let frac = s as f32 / segments as f32;
            let alpha = ctx.effects.matrix_rain.opacity * frac; // brighter at bottom (head)
            if alpha < 0.001 {
                continue;
            }
            let c = Color::new(mr, mg, mb, alpha);
            push_rect(&mut verts, col.x, y, 2.0, seg_h, &c);
        }
    }
    (verts, true)
}

/// Emit frost / ice border effect vertices.
///
/// Draws a crystalline pattern along all four edges of every window using
/// pseudo-random strip heights/widths to create an organic frost look.
pub(super) fn emit_frost_border(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.frost_border.enabled {
        return Vec::new();
    }
    let (fr, fg, fb) = ctx.effects.frost_border.color;
    let bw = ctx.effects.frost_border.width;
    let base_alpha = ctx.effects.frost_border.opacity;
    let mut verts: Vec<RectVertex> = Vec::new();
    for info in &ctx.frame_glyphs.window_infos {
        let b = &info.bounds;
        // Create crystalline pattern along edges using pseudo-random strips
        let segments = 30u32;
        // Top edge
        for s in 0..segments {
            let frac = s as f32 / segments as f32;
            let x = b.x + frac * b.width;
            let h_seed = ((s as u64)
                .wrapping_mul(2654435761)
                .wrapping_add(info.window_id.get() as u64))
                & 0xFF;
            let h = bw * (0.3 + (h_seed as f32 / 255.0) * 0.7);
            let a = base_alpha * (0.4 + (h_seed as f32 / 255.0) * 0.6);
            let seg_w = b.width / segments as f32;
            let c = Color::new(fr, fg, fb, a);
            push_rect(&mut verts, x, b.y, seg_w, h, &c);
        }
        // Bottom edge
        for s in 0..segments {
            let frac = s as f32 / segments as f32;
            let x = b.x + frac * b.width;
            let h_seed = ((s as u64)
                .wrapping_mul(6364136223846793005)
                .wrapping_add(info.window_id.get() as u64))
                & 0xFF;
            let h = bw * (0.3 + (h_seed as f32 / 255.0) * 0.7);
            let a = base_alpha * (0.4 + (h_seed as f32 / 255.0) * 0.6);
            let seg_w = b.width / segments as f32;
            let c = Color::new(fr, fg, fb, a);
            push_rect(&mut verts, x, b.y + b.height - h, seg_w, h, &c);
        }
        // Left edge
        let v_segments = 20u32;
        for s in 0..v_segments {
            let frac = s as f32 / v_segments as f32;
            let y = b.y + frac * b.height;
            let w_seed = ((s as u64)
                .wrapping_mul(1442695040888963407)
                .wrapping_add(info.window_id.get() as u64))
                & 0xFF;
            let w = bw * (0.3 + (w_seed as f32 / 255.0) * 0.7);
            let a = base_alpha * (0.4 + (w_seed as f32 / 255.0) * 0.6);
            let seg_h = b.height / v_segments as f32;
            let c = Color::new(fr, fg, fb, a);
            push_rect(&mut verts, b.x, y, w, seg_h, &c);
        }
        // Right edge
        for s in 0..v_segments {
            let frac = s as f32 / v_segments as f32;
            let y = b.y + frac * b.height;
            let w_seed = ((s as u64)
                .wrapping_mul(3141592653589793238)
                .wrapping_add(info.window_id.get() as u64))
                & 0xFF;
            let w = bw * (0.3 + (w_seed as f32 / 255.0) * 0.7);
            let a = base_alpha * (0.4 + (w_seed as f32 / 255.0) * 0.6);
            let seg_h = b.height / v_segments as f32;
            let c = Color::new(fr, fg, fb, a);
            push_rect(&mut verts, b.x + b.width - w, y, w, seg_h, &c);
        }
    }
    verts
}

/// Emit cursor ripple wave effect vertices.
///
/// Spawns expanding circular ripples at cursor positions on movement.
/// Each ripple is rendered as concentric rings approximated by rect segments,
/// fading as they expand outward.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_ripple_wave(
    ctx: &EffectCtx,
    waves: &mut Vec<RippleWaveEntry>,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.cursor_ripple_wave.enabled {
        return (Vec::new(), false);
    }
    let now = ctx.frame_sample.presentation_time();

    // Detect cursor movement and spawn ripple
    if let Some(anim) = ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        // Spawn a new ripple on each frame where cursor moves (debounced by checking recent entries)
        let should_spawn = waves.is_empty()
            || waves.last().is_none_or(|last| {
                let dx = (cx - last.x).abs();
                let dy = (cy - last.y).abs();
                (dx > 2.0 || dy > 2.0) && now.saturating_since(last.started).as_millis() > 50
            });
        if should_spawn {
            waves.push(RippleWaveEntry {
                x: cx,
                y: cy,
                started: now,
                duration: std::time::Duration::from_millis(
                    ctx.effects.cursor_ripple_wave.duration_ms as u64,
                ),
            });
        }
    }

    // Prune expired
    waves.retain(|e| now.saturating_since(e.started) < e.duration);

    let mut needs_redraw = false;
    let mut verts = Vec::new();
    if !waves.is_empty() {
        let (rr, rg, rb) = ctx.effects.cursor_ripple_wave.color;
        let max_r = ctx.effects.cursor_ripple_wave.max_radius;
        let ring_count = ctx.effects.cursor_ripple_wave.ring_count;
        for entry in waves.iter() {
            let t =
                now.saturating_since(entry.started).as_secs_f32() / entry.duration.as_secs_f32();
            let current_r = max_r * t;
            let fade = 1.0 - t;
            for ring in 0..ring_count {
                let ring_r = current_r * (1.0 - ring as f32 * 0.2);
                if ring_r < 1.0 {
                    continue;
                }
                let ring_alpha = ctx.effects.cursor_ripple_wave.opacity
                    * fade
                    * (1.0 - ring as f32 / ring_count as f32);
                if ring_alpha < 0.001 {
                    continue;
                }
                // Approximate circle with rect strips
                let segments = 32u32;
                for s in 0..segments {
                    let angle = s as f32 * std::f32::consts::TAU / segments as f32;
                    let next_angle = (s + 1) as f32 * std::f32::consts::TAU / segments as f32;
                    let x1 = entry.x + angle.cos() * ring_r;
                    let y1 = entry.y + angle.sin() * ring_r;
                    let x2 = entry.x + next_angle.cos() * ring_r;
                    let y2 = entry.y + next_angle.sin() * ring_r;
                    let mx = x1.min(x2);
                    let my = y1.min(y2);
                    let w = (x1 - x2).abs().max(1.5);
                    let h = (y1 - y2).abs().max(1.5);
                    let c = Color::new(rr, rg, rb, ring_alpha);
                    push_rect(&mut verts, mx, my, w, h, &c);
                }
            }
        }
        needs_redraw = true;
    }
    (verts, needs_redraw)
}
/// Emit cursor lighthouse beam effect vertices.
pub(super) fn emit_cursor_lighthouse_beam(ctx: &EffectCtx) -> Vec<RectVertex> {
    let mut verts = Vec::new();
    if !ctx.effects.cursor_lighthouse.enabled || !ctx.cursor_visible {
        return verts;
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let now = ctx
            .frame_sample
            .since_at_presentation(ctx.aurora_start)
            .as_secs_f32();
        let center_x = anim.x + anim.width / 2.0;
        let center_y = anim.y + anim.height / 2.0;
        let angle = now * ctx.effects.cursor_lighthouse.rotation_speed * std::f32::consts::PI * 2.0;
        let beam_half = (ctx.effects.cursor_lighthouse.beam_width / 2.0).to_radians();
        let beam_len = ctx.effects.cursor_lighthouse.beam_length;
        let (lr, lg, lb) = ctx.effects.cursor_lighthouse.color;
        let lop = ctx.effects.cursor_lighthouse.opacity;
        let segments = 20;
        for seg in 0..segments {
            let t = seg as f32 / segments as f32;
            let dist = t * beam_len;
            let bw = 2.0 * dist * beam_half.sin().max(0.02);
            let dx = angle.cos() * dist;
            let dy = angle.sin() * dist;
            let fade = 1.0 - t;
            let seg_len = beam_len / segments as f32;
            let rx = center_x + dx - bw / 2.0;
            let ry = center_y + dy - seg_len / 2.0;
            let c = Color::new(lr, lg, lb, lop * fade * fade);
            push_rect(&mut verts, rx, ry, bw, seg_len, &c);
        }
    }
    verts
}

/// Emit cursor sonar ping effect vertices.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_sonar_ping(
    ctx: &EffectCtx,
    entries: &mut Vec<SonarPingEntry>,
) -> (Vec<RectVertex>, bool) {
    let mut verts = Vec::new();
    let mut needs_redraw = false;
    if !ctx.effects.cursor_sonar_ping.enabled {
        return (verts, needs_redraw);
    }
    let now = ctx.frame_sample.presentation_time();
    entries.retain(|e| now.saturating_since(e.started) < e.duration);
    let (pr, pg, pb) = ctx.effects.cursor_sonar_ping.color;
    let ring_count = ctx.effects.cursor_sonar_ping.ring_count;
    let max_r = ctx.effects.cursor_sonar_ping.max_radius;
    let pop = ctx.effects.cursor_sonar_ping.opacity;
    for entry in entries.iter() {
        let elapsed = now.saturating_since(entry.started).as_secs_f32();
        let t = elapsed / entry.duration.as_secs_f32();
        for ring_idx in 0..ring_count {
            let ring_t = t - ring_idx as f32 * 0.15;
            if !(0.0..=1.0).contains(&ring_t) {
                continue;
            }
            let radius = ring_t * max_r;
            let fade = 1.0 - ring_t;
            let ring_op = pop * fade * fade;
            let ring_thick = 2.0;
            let seg_count = 24;
            for seg in 0..seg_count {
                let a1 = seg as f32 / seg_count as f32 * std::f32::consts::PI * 2.0;
                let a2 = (seg + 1) as f32 / seg_count as f32 * std::f32::consts::PI * 2.0;
                let x1 = entry.cx + a1.cos() * radius;
                let y1 = entry.cy + a1.sin() * radius;
                let x2 = entry.cx + a2.cos() * radius;
                let y2 = entry.cy + a2.sin() * radius;
                let mx = x1.min(x2);
                let my = y1.min(y2);
                let sw = (x2 - x1).abs().max(ring_thick);
                let sh = (y2 - y1).abs().max(ring_thick);
                let c = Color::new(pr, pg, pb, ring_op);
                push_rect(&mut verts, mx, my, sw, sh, &c);
            }
        }
    }
    if !entries.is_empty() {
        needs_redraw = true;
    }
    (verts, needs_redraw)
}

/// Emit lightning bolt effect vertices.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_lightning_bolt(
    ctx: &EffectCtx,
    last: &mut EventTime,
    segments: &mut Vec<(f32, f32, f32, f32)>,
    age: &mut f32,
) -> (Vec<RectVertex>, bool) {
    let mut verts = Vec::new();
    if !ctx.effects.lightning_bolt.enabled {
        return (verts, false);
    }
    let now = ctx.frame_sample.presentation_time();
    let dt = now.saturating_since(*last).as_secs_f32();
    *last = now;
    *age += dt;
    let bolt_interval = 1.0 / ctx.effects.lightning_bolt.frequency;
    let fw = ctx.surface_width as f32;
    let fh = ctx.surface_height as f32;
    // Generate new bolt if timer expired
    if *age >= bolt_interval || segments.is_empty() {
        *age = 0.0;
        segments.clear();
        let time_seed = now.saturating_since(ctx.aurora_start).as_nanos() as u64;
        // Random start point on frame edge
        let h1 = time_seed.wrapping_mul(2654435761);
        let start_x = (h1 & 0xFFFF) as f32 / 65535.0 * fw;
        let mut x = start_x;
        let mut y = 0.0_f32;
        let seg_count = 8 + ((h1 >> 16) & 7) as u32;
        for i in 0..seg_count {
            let h2 = time_seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(i as u64 * 2654435761);
            let dx = ((h2 & 0xFFFF) as f32 / 65535.0 - 0.5) * 60.0;
            let dy = ((h2 >> 16) & 0xFFFF) as f32 / 65535.0 * (fh / seg_count as f32) + 10.0;
            let nx = (x + dx).clamp(0.0, fw);
            let ny = (y + dy).clamp(0.0, fh);
            segments.push((x, y, nx, ny));
            x = nx;
            y = ny;
        }
    }
    // Render bolt segments
    let (lr, lg, lb) = ctx.effects.lightning_bolt.color;
    let bolt_op = ctx.effects.lightning_bolt.opacity * ctx.effects.lightning_bolt.intensity;
    let fade = (1.0 - *age / (bolt_interval * 0.8)).max(0.0);
    for &(x1, y1, x2, y2) in segments.iter() {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let thick = 2.0;
        let mx = x1.min(x2);
        let my = y1.min(y2);
        let bw = (x2 - x1).abs().max(thick);
        let bh = (y2 - y1).abs().max(thick);
        let c = Color::new(lr, lg, lb, bolt_op * fade);
        push_rect(&mut verts, mx, my, bw, bh, &c);
        // Glow layer
        let gc = Color::new(lr, lg, lb, bolt_op * fade * 0.3);
        push_rect(&mut verts, mx - 2.0, my - 2.0, bw + 4.0, bh + 4.0, &gc);
        let _ = len; // suppress unused warning
    }
    (verts, true)
}

/// Emit cursor orbit particles effect vertices.
pub(super) fn emit_cursor_orbit_particles(ctx: &EffectCtx) -> Vec<RectVertex> {
    let mut verts = Vec::new();
    if !ctx.effects.cursor_orbit_particles.enabled || !ctx.cursor_visible {
        return verts;
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let now = ctx
            .frame_sample
            .since_at_presentation(ctx.aurora_start)
            .as_secs_f32();
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        let (pr, pg, pb) = ctx.effects.cursor_orbit_particles.color;
        let pop = ctx.effects.cursor_orbit_particles.opacity;
        let count = ctx.effects.cursor_orbit_particles.count;
        let radius = ctx.effects.cursor_orbit_particles.radius;
        let speed = ctx.effects.cursor_orbit_particles.speed;
        for i in 0..count {
            let phase = i as f32 / count as f32 * std::f32::consts::PI * 2.0;
            let angle = now * speed * std::f32::consts::PI * 2.0 + phase;
            let r_var = radius * (1.0 + 0.2 * (now * 2.0 + phase).sin());
            let px = cx + angle.cos() * r_var;
            let py = cy + angle.sin() * r_var;
            let size = 3.0;
            let c = Color::new(pr, pg, pb, pop);
            push_rect(&mut verts, px - size / 2.0, py - size / 2.0, size, size, &c);
            // Small glow
            let gc = Color::new(pr, pg, pb, pop * 0.3);
            push_rect(
                &mut verts,
                px - size,
                py - size,
                size * 2.0,
                size * 2.0,
                &gc,
            );
        }
    }
    verts
}

/// Emit cursor heartbeat pulse effect vertices.
pub(super) fn emit_cursor_heartbeat_pulse(ctx: &EffectCtx) -> Vec<RectVertex> {
    let mut verts = Vec::new();
    if !ctx.effects.cursor_heartbeat.enabled || !ctx.cursor_visible {
        return verts;
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let now = ctx
            .frame_sample
            .since_at_presentation(ctx.aurora_start)
            .as_secs_f32();
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        let (hr, hg, hb) = ctx.effects.cursor_heartbeat.color;
        let hop = ctx.effects.cursor_heartbeat.opacity;
        let max_r = ctx.effects.cursor_heartbeat.max_radius;
        let beat_period = 60.0 / ctx.effects.cursor_heartbeat.bpm;
        let t_in_beat = (now % beat_period) / beat_period;
        // Double pulse: first at t=0.0-0.15, second at t=0.2-0.35
        let pulses = [(0.0_f32, 0.15_f32), (0.2, 0.35)];
        for &(start, end) in &pulses {
            if t_in_beat >= start && t_in_beat <= end {
                let pt = (t_in_beat - start) / (end - start);
                let radius = pt * max_r;
                let fade = 1.0 - pt;
                let ring_thick = 2.0;
                let seg_count = 20;
                for seg in 0..seg_count {
                    let a1 = seg as f32 / seg_count as f32 * std::f32::consts::PI * 2.0;
                    let a2 = (seg + 1) as f32 / seg_count as f32 * std::f32::consts::PI * 2.0;
                    let x1 = cx + a1.cos() * radius;
                    let y1 = cy + a1.sin() * radius;
                    let x2 = cx + a2.cos() * radius;
                    let y2 = cy + a2.sin() * radius;
                    let mx = x1.min(x2);
                    let my = y1.min(y2);
                    let sw = (x2 - x1).abs().max(ring_thick);
                    let sh = (y2 - y1).abs().max(ring_thick);
                    let c = Color::new(hr, hg, hb, hop * fade * fade);
                    push_rect(&mut verts, mx, my, sw, sh, &c);
                }
            }
        }
    }
    verts
}

/// Emit cursor metronome tick effect vertices.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_metronome_tick(
    ctx: &EffectCtx,
    last_x: &mut f32,
    last_y: &mut f32,
    tick_start: &mut Option<EventTime>,
) -> (Vec<RectVertex>, bool) {
    let mut verts = Vec::new();
    let mut needs_redraw = false;
    if !ctx.effects.cursor_metronome.enabled || !ctx.cursor_visible {
        return (verts, needs_redraw);
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y;
        // Detect cursor move
        if (cx - *last_x).abs() > 1.0 || (cy - *last_y).abs() > 1.0 {
            *tick_start = Some(ctx.frame_sample.presentation_time());
            *last_x = cx;
            *last_y = cy;
        }
        if let Some(start) = *tick_start {
            let elapsed = ctx.frame_sample.since_at_presentation(start).as_secs_f32();
            let duration = ctx.effects.cursor_metronome.fade_ms as f32 / 1000.0;
            if elapsed < duration {
                let t = elapsed / duration;
                let fade = 1.0 - t;
                let (mr, mg, mb) = ctx.effects.cursor_metronome.color;
                let mop = ctx.effects.cursor_metronome.opacity;
                let th = ctx.effects.cursor_metronome.tick_height;
                // Vertical tick line above cursor
                let tick_w = 2.0;
                let c = Color::new(mr, mg, mb, mop * fade);
                push_rect(
                    &mut verts,
                    cx - tick_w / 2.0,
                    cy - th * (1.0 - t * 0.3),
                    tick_w,
                    th,
                    &c,
                );
                // Small horizontal cap
                let cap_w = 6.0;
                let cc = Color::new(mr, mg, mb, mop * fade * 0.8);
                push_rect(
                    &mut verts,
                    cx - cap_w / 2.0,
                    cy - th * (1.0 - t * 0.3),
                    cap_w,
                    1.5,
                    &cc,
                );
                needs_redraw = true;
            } else {
                *tick_start = None;
            }
        }
    }
    (verts, needs_redraw)
}

/// Emit cursor radar sweep effect vertices.
pub(super) fn emit_cursor_radar_sweep(ctx: &EffectCtx) -> Vec<RectVertex> {
    let mut verts = Vec::new();
    if !ctx.effects.cursor_radar.enabled || !ctx.cursor_visible {
        return verts;
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let now = ctx
            .frame_sample
            .since_at_presentation(ctx.aurora_start)
            .as_secs_f32();
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        let (rr, rg, rb) = ctx.effects.cursor_radar.color;
        let rop = ctx.effects.cursor_radar.opacity;
        let radius = ctx.effects.cursor_radar.radius;
        let spd = ctx.effects.cursor_radar.speed;
        let sweep_angle = now * spd * std::f32::consts::PI * 2.0;
        // Sweep beam line
        let beam_segs = 15;
        for seg in 0..beam_segs {
            let t = seg as f32 / beam_segs as f32;
            let r = t * radius;
            let px = cx + sweep_angle.cos() * r;
            let py = cy + sweep_angle.sin() * r;
            let dot = 2.0;
            let c = Color::new(rr, rg, rb, rop * (1.0 - t * 0.5));
            push_rect(&mut verts, px - dot / 2.0, py - dot / 2.0, dot, dot, &c);
        }
        // Fading trail (previous sweep positions)
        let trail_count = 20;
        for trail in 1..trail_count {
            let trail_angle = sweep_angle - trail as f32 * 0.08;
            let fade = 1.0 - trail as f32 / trail_count as f32;
            for seg in 0..8 {
                let t = seg as f32 / 8.0;
                let r = t * radius;
                let px = cx + trail_angle.cos() * r;
                let py = cy + trail_angle.sin() * r;
                let dot = 1.5;
                let c = Color::new(rr, rg, rb, rop * fade * fade * 0.3);
                push_rect(&mut verts, px - dot / 2.0, py - dot / 2.0, dot, dot, &c);
            }
        }
        // Concentric range rings
        let ring_count = 3;
        for ring in 1..=ring_count {
            let ring_r = radius * ring as f32 / ring_count as f32;
            let ring_segs = 24;
            for seg in 0..ring_segs {
                let a = seg as f32 / ring_segs as f32 * std::f32::consts::PI * 2.0;
                let px = cx + a.cos() * ring_r;
                let py = cy + a.sin() * ring_r;
                let c = Color::new(rr, rg, rb, rop * 0.15);
                push_rect(&mut verts, px - 0.5, py - 0.5, 1.0, 1.0, &c);
            }
        }
    }
    verts
}

/// Emit cursor ripple ring effect vertices.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_ripple_ring(
    ctx: &EffectCtx,
    start: &mut Option<EventTime>,
    last_x: &mut f32,
    last_y: &mut f32,
) -> (Vec<RectVertex>, bool) {
    let mut verts = Vec::new();
    let mut needs_redraw = false;
    if !ctx.effects.cursor_ripple_ring.enabled || !ctx.cursor_visible {
        return (verts, needs_redraw);
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        // Detect cursor move
        if (cx - *last_x).abs() > 1.0 || (cy - *last_y).abs() > 1.0 {
            *start = Some(ctx.frame_sample.presentation_time());
            *last_x = cx;
            *last_y = cy;
        }
        if let Some(start_time) = *start {
            let elapsed = ctx
                .frame_sample
                .since_at_presentation(start_time)
                .as_secs_f32();
            let max_r = ctx.effects.cursor_ripple_ring.max_radius;
            let duration = max_r / (ctx.effects.cursor_ripple_ring.speed * 60.0);
            if elapsed < duration {
                let t = elapsed / duration;
                let (rr, rg, rb) = ctx.effects.cursor_ripple_ring.color;
                let rop = ctx.effects.cursor_ripple_ring.opacity;
                let count = ctx.effects.cursor_ripple_ring.count.clamp(1, 8);
                for ring in 0..count {
                    let ring_t = (t - ring as f32 * 0.15).clamp(0.0, 1.0);
                    if ring_t <= 0.0 {
                        continue;
                    }
                    let radius = ring_t * max_r;
                    let fade = (1.0 - ring_t) * rop;
                    let ring_segs = 32;
                    for seg in 0..ring_segs {
                        let a = seg as f32 / ring_segs as f32 * std::f32::consts::PI * 2.0;
                        let px = cx + a.cos() * radius;
                        let py = cy + a.sin() * radius;
                        let dot = 2.0;
                        let c = Color::new(rr, rg, rb, fade);
                        push_rect(&mut verts, px - dot / 2.0, py - dot / 2.0, dot, dot, &c);
                    }
                }
                needs_redraw = true;
            } else {
                *start = None;
            }
        }
    }
    (verts, needs_redraw)
}

/// Emit cursor scope effect vertices.
pub(super) fn emit_cursor_scope(ctx: &EffectCtx) -> Vec<RectVertex> {
    let mut verts = Vec::new();
    if !ctx.effects.cursor_scope.enabled || !ctx.cursor_visible {
        return verts;
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        let (chr, chg, chb) = ctx.effects.cursor_scope.color;
        let chop = ctx.effects.cursor_scope.opacity;
        let thick = ctx.effects.cursor_scope.thickness;
        let gap = ctx.effects.cursor_scope.gap;
        let fw = ctx.surface_width as f32;
        let fh = ctx.surface_height as f32;
        let c = Color::new(chr, chg, chb, chop);
        // Horizontal line -- left of cursor
        if cx - gap > 0.0 {
            push_rect(&mut verts, 0.0, cy - thick / 2.0, cx - gap, thick, &c);
        }
        // Horizontal line -- right of cursor
        if cx + gap < fw {
            push_rect(
                &mut verts,
                cx + gap,
                cy - thick / 2.0,
                fw - (cx + gap),
                thick,
                &c,
            );
        }
        // Vertical line -- above cursor
        if cy - gap > 0.0 {
            push_rect(&mut verts, cx - thick / 2.0, 0.0, thick, cy - gap, &c);
        }
        // Vertical line -- below cursor
        if cy + gap < fh {
            push_rect(
                &mut verts,
                cx - thick / 2.0,
                cy + gap,
                thick,
                fh - (cy + gap),
                &c,
            );
        }
    }
    verts
}

/// Emit cursor shockwave effect vertices.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_shockwave(
    ctx: &EffectCtx,
    start: &mut Option<EventTime>,
    last_x: &mut f32,
    last_y: &mut f32,
) -> (Vec<RectVertex>, bool) {
    let mut verts = Vec::new();
    let mut needs_redraw = false;
    if !ctx.effects.cursor_shockwave.enabled || !ctx.cursor_visible {
        return (verts, needs_redraw);
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        // Detect cursor move
        if (cx - *last_x).abs() > 1.0 || (cy - *last_y).abs() > 1.0 {
            *start = Some(ctx.frame_sample.presentation_time());
            *last_x = cx;
            *last_y = cy;
        }
        if let Some(start_time) = *start {
            let elapsed = ctx
                .frame_sample
                .since_at_presentation(start_time)
                .as_secs_f32();
            let max_r = ctx.effects.cursor_shockwave.radius;
            let duration = 1.0 / ctx.effects.cursor_shockwave.decay;
            if elapsed < duration {
                let t = elapsed / duration;
                let radius = t * max_r;
                let (sr, sg, sb) = ctx.effects.cursor_shockwave.color;
                let sop = ctx.effects.cursor_shockwave.opacity;
                let fade = (1.0 - t) * (1.0 - t);
                // Ring at current radius
                let ring_thick = 3.0 * (1.0 - t * 0.5);
                let ring_segs = 48;
                for seg in 0..ring_segs {
                    let a = seg as f32 / ring_segs as f32 * std::f32::consts::PI * 2.0;
                    let px = cx + a.cos() * radius;
                    let py = cy + a.sin() * radius;
                    let c = Color::new(sr, sg, sb, sop * fade);
                    push_rect(
                        &mut verts,
                        px - ring_thick / 2.0,
                        py - ring_thick / 2.0,
                        ring_thick,
                        ring_thick,
                        &c,
                    );
                }
                // Inner glow
                let inner_r = radius * 0.7;
                for seg in 0..24 {
                    let a = seg as f32 / 24.0 * std::f32::consts::PI * 2.0;
                    let px = cx + a.cos() * inner_r;
                    let py = cy + a.sin() * inner_r;
                    let c = Color::new(sr, sg, sb, sop * fade * 0.3);
                    push_rect(&mut verts, px - 1.0, py - 1.0, 2.0, 2.0, &c);
                }
                needs_redraw = true;
            } else {
                *start = None;
            }
        }
    }
    (verts, needs_redraw)
}

/// Emit cursor gravity well effect vertices.
pub(super) fn emit_cursor_gravity_well(ctx: &EffectCtx) -> Vec<RectVertex> {
    let mut verts = Vec::new();
    if !ctx.effects.cursor_gravity_well.enabled || !ctx.cursor_visible {
        return verts;
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        let (gr, gg, gb) = ctx.effects.cursor_gravity_well.color;
        let gop = ctx.effects.cursor_gravity_well.opacity;
        let field_r = ctx.effects.cursor_gravity_well.field_radius;
        let lines = ctx.effects.cursor_gravity_well.line_count.clamp(4, 24);
        let now = ctx
            .frame_sample
            .since_at_presentation(ctx.aurora_start)
            .as_secs_f32();
        for line in 0..lines {
            let base_angle = line as f32 * std::f32::consts::PI * 2.0 / lines as f32 + now * 0.2;
            let steps = 20;
            for step in 0..steps {
                let t = step as f32 / steps as f32;
                let r = field_r * (1.0 - t);
                // Spiral inward with slight curve
                let curve = t * t * 1.5;
                let angle = base_angle + curve;
                let px = cx + angle.cos() * r;
                let py = cy + angle.sin() * r;
                let sz = 1.5 + t * 2.0;
                let alpha = gop * t * t;
                let c = Color::new(gr, gg, gb, alpha);
                push_rect(&mut verts, px - sz / 2.0, py - sz / 2.0, sz, sz, &c);
            }
        }
    }
    verts
}

/// Emit cursor portal effect vertices.
pub(super) fn emit_cursor_portal(ctx: &EffectCtx) -> Vec<RectVertex> {
    let mut verts = Vec::new();
    if !ctx.effects.cursor_portal.enabled || !ctx.cursor_visible {
        return verts;
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let now = ctx
            .frame_sample
            .since_at_presentation(ctx.aurora_start)
            .as_secs_f32();
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        let (pr, pg, pb) = ctx.effects.cursor_portal.color;
        let pop = ctx.effects.cursor_portal.opacity;
        let radius = ctx.effects.cursor_portal.radius;
        let spd = ctx.effects.cursor_portal.speed;
        // Outer swirling ring
        let ring_segs = 36;
        for seg in 0..ring_segs {
            let a = seg as f32 / ring_segs as f32 * std::f32::consts::PI * 2.0;
            let wobble = (a * 3.0 + now * spd * 2.0).sin() * 3.0;
            let r = radius + wobble;
            let px = cx + a.cos() * r;
            let py = cy + a.sin() * r;
            let sz = 2.5 + (a * 5.0 + now * spd * 3.0).sin().abs() * 1.5;
            let alpha = pop * (0.6 + 0.4 * (a * 4.0 + now * spd).sin());
            let c = Color::new(pr, pg, pb, alpha);
            push_rect(&mut verts, px - sz / 2.0, py - sz / 2.0, sz, sz, &c);
        }
        // Inner spiral
        let spiral_segs = 20;
        for seg in 0..spiral_segs {
            let t = seg as f32 / spiral_segs as f32;
            let a = t * std::f32::consts::PI * 4.0 + now * spd * 3.0;
            let r = radius * (1.0 - t) * 0.8;
            let px = cx + a.cos() * r;
            let py = cy + a.sin() * r;
            let sz = 1.5 * (1.0 - t);
            let c = Color::new(pr, pg, pb, pop * t * 0.5);
            push_rect(&mut verts, px - sz / 2.0, py - sz / 2.0, sz, sz, &c);
        }
    }
    verts
}

/// Emit cursor bubble effect vertices.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_bubble(
    ctx: &EffectCtx,
    spawn_time: &mut Option<EventTime>,
    last_x: &mut f32,
    last_y: &mut f32,
) -> (Vec<RectVertex>, bool) {
    let mut verts = Vec::new();
    let mut needs_redraw = false;
    if !ctx.effects.cursor_bubble.enabled || !ctx.cursor_visible {
        return (verts, needs_redraw);
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        // Detect cursor move
        if (cx - *last_x).abs() > 1.0 || (cy - *last_y).abs() > 1.0 {
            *spawn_time = Some(ctx.frame_sample.presentation_time());
            *last_x = cx;
            *last_y = cy;
        }
        if let Some(spawn) = *spawn_time {
            let elapsed = ctx.frame_sample.since_at_presentation(spawn).as_secs_f32();
            let duration = 1.5;
            if elapsed < duration {
                let (br, bg, bb) = ctx.effects.cursor_bubble.color;
                let bop = ctx.effects.cursor_bubble.opacity;
                let count = ctx.effects.cursor_bubble.count.clamp(2, 12);
                let rspd = ctx.effects.cursor_bubble.rise_speed;
                for i in 0..count {
                    let mut h = i.wrapping_mul(2654435761);
                    h ^= h >> 16;
                    let offset_x = (h as f32 / u32::MAX as f32 - 0.5) * 20.0;
                    h = h.wrapping_mul(0x45d9f3b);
                    h ^= h >> 16;
                    let offset_delay = (h as f32 / u32::MAX as f32) * 0.3;
                    let t = (elapsed - offset_delay).max(0.0) / duration;
                    if t <= 0.0 || t >= 1.0 {
                        continue;
                    }
                    let rise = t * rspd;
                    let bx = cx + offset_x + (t * 5.0 + i as f32).sin() * 4.0;
                    let by = cy - rise;
                    let sz = (3.0 + i as f32 * 0.5) * (1.0 - t * 0.5);
                    let fade = (1.0 - t) * bop;
                    let c = Color::new(br, bg, bb, fade);
                    // Draw circle approximation with ring of dots
                    let ring = 8;
                    for r in 0..ring {
                        let a = r as f32 / ring as f32 * std::f32::consts::PI * 2.0;
                        let px = bx + a.cos() * sz;
                        let py = by + a.sin() * sz;
                        push_rect(&mut verts, px - 0.75, py - 0.75, 1.5, 1.5, &c);
                    }
                }
                needs_redraw = true;
            } else {
                *spawn_time = None;
            }
        }
    }
    (verts, needs_redraw)
}

/// Emit cursor firework effect vertices.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_firework(
    ctx: &EffectCtx,
    start: &mut Option<EventTime>,
    last_x: &mut f32,
    last_y: &mut f32,
) -> (Vec<RectVertex>, bool) {
    let mut verts = Vec::new();
    let mut needs_redraw = false;
    if !ctx.effects.cursor_firework.enabled || !ctx.cursor_visible {
        return (verts, needs_redraw);
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        // Detect movement
        let dx = cx - *last_x;
        let dy = cy - *last_y;
        if dx.abs() > 1.0 || dy.abs() > 1.0 {
            *start = Some(ctx.frame_sample.presentation_time());
            *last_x = cx;
            *last_y = cy;
        }
        if let Some(start_time) = *start {
            let elapsed = ctx
                .frame_sample
                .since_at_presentation(start_time)
                .as_secs_f32();
            let duration = 0.6;
            if elapsed < duration {
                let t = elapsed / duration;
                let (cr, cg, cb) = ctx.effects.cursor_firework.color;
                let opacity = ctx.effects.cursor_firework.opacity;
                let radius = ctx.effects.cursor_firework.burst_radius;
                for i in 0..ctx.effects.cursor_firework.particle_count {
                    let angle = (i as f32 / ctx.effects.cursor_firework.particle_count as f32)
                        * std::f32::consts::TAU;
                    // Add some variation using deterministic hash
                    let mut h = i.wrapping_mul(2654435761);
                    h ^= h >> 16;
                    let speed_var = 0.7 + (h % 60) as f32 / 100.0;
                    let dist = t * radius * speed_var;
                    let px = cx + angle.cos() * dist;
                    let py = cy + angle.sin() * dist;
                    let size = 3.0 * (1.0 - t);
                    let alpha = opacity * (1.0 - t * t);
                    let c = Color::new(cr, cg, cb, alpha);
                    push_rect(&mut verts, px - size / 2.0, py - size / 2.0, size, size, &c);
                }
                needs_redraw = true;
            } else {
                *start = None;
            }
        }
    }
    (verts, needs_redraw)
}

/// Emit cursor tornado effect vertices.
pub(super) fn emit_cursor_tornado(ctx: &EffectCtx) -> Vec<RectVertex> {
    let mut verts = Vec::new();
    if !ctx.effects.cursor_tornado.enabled || !ctx.cursor_visible {
        return verts;
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        let now = ctx
            .frame_sample
            .since_at_presentation(ctx.aurora_start)
            .as_secs_f32();
        let (cr, cg, cb) = ctx.effects.cursor_tornado.color;
        let radius = ctx.effects.cursor_tornado.radius;
        let opacity = ctx.effects.cursor_tornado.opacity;
        for i in 0..ctx.effects.cursor_tornado.particle_count {
            let t = i as f32 / ctx.effects.cursor_tornado.particle_count as f32;
            // Particles spiral inward at different heights
            let angle = now * 3.0 + t * std::f32::consts::TAU * 2.0;
            let r = radius * (1.0 - t * 0.7);
            let px = cx + angle.cos() * r;
            let py = cy + angle.sin() * r - t * radius * 0.5;
            let size = 3.0 * (1.0 - t * 0.5);
            let alpha = opacity * (1.0 - t * 0.6);
            let c = Color::new(cr, cg, cb, alpha);
            push_rect(&mut verts, px - size / 2.0, py - size / 2.0, size, size, &c);
        }
        // Add funnel outline rings
        let ring_count = 5u32;
        for r_idx in 0..ring_count {
            let t = r_idx as f32 / ring_count as f32;
            let ring_r = radius * (1.0 - t * 0.6);
            let ring_y = cy - t * radius * 0.4;
            let segments = 24u32;
            for s in 0..segments {
                let angle =
                    (s as f32 / segments as f32) * std::f32::consts::TAU + now * 2.0 * (1.0 + t);
                let px = cx + angle.cos() * ring_r;
                let py = ring_y + angle.sin() * ring_r * 0.3;
                let alpha = opacity * 0.4 * (1.0 - t);
                let c = Color::new(cr, cg, cb, alpha);
                push_rect(&mut verts, px - 1.0, py - 1.0, 2.0, 2.0, &c);
            }
        }
    }
    verts
}

/// Emit cursor lightning effect vertices.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_lightning(
    ctx: &EffectCtx,
    start: &mut Option<EventTime>,
    last_x: &mut f32,
    last_y: &mut f32,
) -> (Vec<RectVertex>, bool) {
    let mut verts = Vec::new();
    let mut needs_redraw = false;
    if !ctx.effects.cursor_lightning.enabled || !ctx.cursor_visible {
        return (verts, needs_redraw);
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        let dx = cx - *last_x;
        let dy = cy - *last_y;
        if dx.abs() > 1.0 || dy.abs() > 1.0 {
            *start = Some(ctx.frame_sample.presentation_time());
            *last_x = cx;
            *last_y = cy;
        }
        if let Some(start_time) = *start {
            let elapsed = ctx
                .frame_sample
                .since_at_presentation(start_time)
                .as_secs_f32();
            let duration = 0.4;
            if elapsed < duration {
                let t = elapsed / duration;
                let (cr, cg, cb) = ctx.effects.cursor_lightning.color;
                let opacity = ctx.effects.cursor_lightning.opacity;
                let max_len = ctx.effects.cursor_lightning.max_length;
                for bolt in 0..ctx.effects.cursor_lightning.bolt_count {
                    let mut h = bolt.wrapping_mul(2654435761);
                    h ^= h >> 16;
                    let angle = (h % 360) as f32 * std::f32::consts::PI / 180.0;
                    let segments = 6u32;
                    let mut px = cx;
                    let mut py = cy;
                    for seg in 0..segments {
                        let seg_len = max_len / segments as f32;
                        let jitter_angle =
                            angle + ((h.wrapping_mul(seg + 1) >> 8) % 60) as f32 * 0.02 - 0.6;
                        let nx = px + jitter_angle.cos() * seg_len;
                        let ny = py + jitter_angle.sin() * seg_len;
                        let alpha =
                            opacity * (1.0 - t) * (1.0 - seg as f32 / segments as f32 * 0.5);
                        let thickness = 2.0 * (1.0 - t * 0.5);
                        let c = Color::new(cr, cg, cb, alpha);
                        // Draw segment as small rect
                        let mx = (px + nx) / 2.0;
                        let my = (py + ny) / 2.0;
                        push_rect(
                            &mut verts,
                            mx - thickness / 2.0,
                            my - thickness / 2.0,
                            thickness,
                            seg_len.max(thickness),
                            &c,
                        );
                        px = nx;
                        py = ny;
                        h = h.wrapping_mul(1103515245).wrapping_add(12345);
                    }
                }
                needs_redraw = true;
            } else {
                *start = None;
            }
        }
    }
    (verts, needs_redraw)
}

/// Emit cursor snowflake effect vertices.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_snowflake(
    ctx: &EffectCtx,
    start: &mut Option<EventTime>,
    last_x: &mut f32,
    last_y: &mut f32,
) -> (Vec<RectVertex>, bool) {
    let mut verts = Vec::new();
    let mut needs_redraw = false;
    if !ctx.effects.cursor_snowflake.enabled || !ctx.cursor_visible {
        return (verts, needs_redraw);
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y + anim.height / 2.0;
        let dx = cx - *last_x;
        let dy = cy - *last_y;
        if dx.abs() > 1.0 || dy.abs() > 1.0 {
            *start = Some(ctx.frame_sample.presentation_time());
            *last_x = cx;
            *last_y = cy;
        }
        if let Some(start_time) = *start {
            let elapsed = ctx
                .frame_sample
                .since_at_presentation(start_time)
                .as_secs_f32();
            let duration = 2.0;
            if elapsed < duration {
                let t = elapsed / duration;
                let (cr, cg, cb) = ctx.effects.cursor_snowflake.color;
                let opacity = ctx.effects.cursor_snowflake.opacity;
                let fall_speed = ctx.effects.cursor_snowflake.fall_speed;
                for i in 0..ctx.effects.cursor_snowflake.count {
                    let mut h = i.wrapping_mul(2654435761);
                    h ^= h >> 16;
                    let offset_x = (h % 60) as f32 - 30.0;
                    let drift = ((h >> 8) % 20) as f32 * 0.1 - 1.0;
                    let fall_y = elapsed * fall_speed * (0.5 + (h % 100) as f32 / 100.0);
                    let px = cx + offset_x + drift * elapsed * 5.0;
                    let py = cy + fall_y;
                    let size = 3.0 + (h % 3) as f32;
                    let alpha = opacity * (1.0 - t);
                    let c = Color::new(cr, cg, cb, alpha);
                    // Simple snowflake: center + 6 arms as small rects
                    push_rect(&mut verts, px - size / 2.0, py - 0.5, size, 1.0, &c);
                    push_rect(&mut verts, px - 0.5, py - size / 2.0, 1.0, size, &c);
                    // Diagonal arms approximated
                    let arm = size * 0.35;
                    push_rect(&mut verts, px - arm, py - arm, arm * 0.7, arm * 0.7, &c);
                    push_rect(
                        &mut verts,
                        px + arm * 0.3,
                        py + arm * 0.3,
                        arm * 0.7,
                        arm * 0.7,
                        &c,
                    );
                }
                needs_redraw = true;
            } else {
                *start = None;
            }
        }
    }
    (verts, needs_redraw)
}

/// Emit cursor flame effect vertices.
pub(super) fn emit_cursor_flame(ctx: &EffectCtx) -> Vec<RectVertex> {
    let mut verts = Vec::new();
    if !ctx.effects.cursor_flame.enabled || !ctx.cursor_visible {
        return verts;
    }
    if let Some(ref anim) = *ctx.animated_cursor {
        let cx = anim.x + anim.width / 2.0;
        let cy = anim.y;
        let now = ctx
            .frame_sample
            .since_at_presentation(ctx.aurora_start)
            .as_secs_f32();
        let (cr, cg, cb) = ctx.effects.cursor_flame.color;
        let opacity = ctx.effects.cursor_flame.opacity;
        let flame_h = ctx.effects.cursor_flame.height;
        for i in 0..ctx.effects.cursor_flame.particle_count {
            let mut h = i.wrapping_mul(2654435761);
            h ^= h >> 16;
            let t_offset = (h % 100) as f32 / 100.0;
            let flicker = (now * 8.0 + t_offset * std::f32::consts::TAU).sin() * 0.5 + 0.5;
            let rise = (now * 3.0 + t_offset * 10.0) % 1.0;
            let px = cx + ((h % 20) as f32 - 10.0) * flicker;
            let py = cy - rise * flame_h;
            let size = 4.0 * (1.0 - rise) * flicker;
            // Warm color gradient: red at base, yellow at tip
            let rr = cr;
            let gg = cg + rise * 0.3;
            let bb = cb * (1.0 - rise);
            let alpha = opacity * (1.0 - rise);
            let c = Color::new(rr, gg.min(1.0), bb.max(0.0), alpha);
            push_rect(&mut verts, px - size / 2.0, py - size / 2.0, size, size, &c);
        }
    }
    verts
}
/// Emit cursor crystal effect vertices.
pub(super) fn emit_cursor_crystal(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_crystal.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let (cr, cg, cb) = ctx.effects.cursor_crystal.color;
    let opacity = ctx.effects.cursor_crystal.opacity;
    let radius = ctx.effects.cursor_crystal.radius;
    let facets = ctx.effects.cursor_crystal.facet_count.max(3);
    let mut verts: Vec<RectVertex> = Vec::new();
    // Draw faceted crystal shape
    for i in 0..facets {
        let angle = (i as f32 / facets as f32) * std::f32::consts::TAU + now * 0.5;
        let next_angle = ((i + 1) as f32 / facets as f32) * std::f32::consts::TAU + now * 0.5;
        // Draw edge line from vertex to next vertex
        let x1 = cx + angle.cos() * radius;
        let y1 = cy + angle.sin() * radius;
        let x2 = cx + next_angle.cos() * radius;
        let y2 = cy + next_angle.sin() * radius;
        let segments = 8u32;
        for s in 0..segments {
            let t = s as f32 / segments as f32;
            let px = x1 + (x2 - x1) * t;
            let py = y1 + (y2 - y1) * t;
            let c = Color::new(cr, cg, cb, opacity);
            push_rect(&mut verts, px - 1.0, py - 1.0, 2.0, 2.0, &c);
        }
        // Draw line from center to vertex (inner facet)
        for s in 0..6u32 {
            let t = s as f32 / 6.0;
            let px = cx + (x1 - cx) * t;
            let py = cy + (y1 - cy) * t;
            let alpha = opacity * 0.4 * (1.0 - t);
            let c = Color::new(cr, cg, cb, alpha);
            push_rect(&mut verts, px - 0.5, py - 0.5, 1.0, 1.0, &c);
        }
    }
    verts
}

/// Emit cursor water drop effect vertices.
pub(super) fn emit_cursor_water_drop(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_water_drop.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let (wr, wg, wb) = ctx.effects.cursor_water_drop.color;
    let ripple_count = ctx.effects.cursor_water_drop.ripple_count;
    let speed = ctx.effects.cursor_water_drop.expand_speed;
    let alpha = ctx.effects.cursor_water_drop.opacity;
    let mut verts = Vec::new();

    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;

    for i in 0..ripple_count {
        let phase = (now * speed + i as f32 * 0.5) % 2.0;
        let r = phase * 30.0;
        let fade = (1.0 - phase / 2.0).max(0.0);
        let segments = 24;

        for s in 0..segments {
            let angle = (s as f32 / segments as f32) * std::f32::consts::TAU;
            let px = cx + angle.cos() * r;
            let py = cy + angle.sin() * r;
            let c = Color::new(wr, wg, wb, alpha * fade);
            push_rect(&mut verts, px - 1.0, py - 1.0, 2.0, 2.0, &c);
        }
    }
    verts
}

/// Emit cursor pixel dust effect vertices.
pub(super) fn emit_cursor_pixel_dust(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_pixel_dust.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let (pr, pg, pb) = ctx.effects.cursor_pixel_dust.color;
    let dust_count = ctx.effects.cursor_pixel_dust.count;
    let scatter = ctx.effects.cursor_pixel_dust.scatter_speed;
    let alpha = ctx.effects.cursor_pixel_dust.opacity;
    let mut verts = Vec::new();

    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;

    for i in 0..dust_count {
        let seed = i as f32 * 7.31;
        let life = (now * scatter + seed) % 2.0;
        let angle = seed * 2.39996; // Golden angle
        let speed_mult = 0.5 + (seed % 1.0) * 1.5;

        let dx = angle.cos() * life * 20.0 * speed_mult;
        let dy = angle.sin() * life * 20.0 * speed_mult - life * 5.0; // Slight upward drift
        let px = cx + dx;
        let py = cy + dy;

        let fade = (1.0 - life / 2.0).max(0.0);
        let pixel_size = 3.0 * fade + 1.0;

        let c = Color::new(pr, pg, pb, alpha * fade);
        push_rect(
            &mut verts,
            px - pixel_size / 2.0,
            py - pixel_size / 2.0,
            pixel_size,
            pixel_size,
            &c,
        );
    }
    verts
}

/// Emit cursor candle flame effect vertices.
pub(super) fn emit_cursor_candle_flame(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_candle_flame.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let (fr, fg, fb) = ctx.effects.cursor_candle_flame.color;
    let flame_h = ctx.effects.cursor_candle_flame.height as f32;
    let flicker = ctx.effects.cursor_candle_flame.flicker_speed;
    let alpha = ctx.effects.cursor_candle_flame.opacity;
    let mut verts = Vec::new();

    let cx = anim.x + anim.width / 2.0;
    let base_y = anim.y;

    // Flame body (tapers upward)
    let layers = 12;
    for i in 0..layers {
        let t = i as f32 / layers as f32;
        let y = base_y - t * flame_h;
        let w = anim.width * (1.0 - t * 0.8) * (1.0 + (now * flicker * 8.0 + t * 3.0).sin() * 0.2);
        let fade = (1.0 - t).powf(0.5);

        // Color shifts from yellow at base to orange/red at tip
        let cr = fr;
        let cg = fg * (1.0 - t * 0.6);
        let cb = fb * (1.0 - t);

        let wobble = (now * flicker * 6.0 + t * 5.0).sin() * 2.0 * t;
        let c = Color::new(cr, cg, cb, alpha * fade);
        push_rect(
            &mut verts,
            cx - w / 2.0 + wobble,
            y,
            w,
            flame_h / layers as f32 + 1.0,
            &c,
        );
    }

    // Wax drip particles
    for d in 0..3 {
        let drip_t = (now * flicker * 0.5 + d as f32 * 1.1) % 2.0;
        if drip_t < 1.5 {
            let dy = base_y + anim.height + drip_t * 10.0;
            let dx = cx + (d as f32 - 1.0) * 3.0;
            let fade = (1.0 - drip_t / 1.5).max(0.0);
            let c = Color::new(0.9, 0.85, 0.7, alpha * fade * 0.5);
            push_rect(&mut verts, dx - 1.0, dy, 2.0, 3.0, &c);
        }
    }
    verts
}

/// Emit cursor moth flame effect vertices.
pub(super) fn emit_cursor_moth_flame(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_moth_flame.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let (mr, mg, mb) = ctx.effects.cursor_moth_flame.color;
    let moth_count = ctx.effects.cursor_moth_flame.moth_count;
    let orbit = ctx.effects.cursor_moth_flame.orbit_speed;
    let alpha = ctx.effects.cursor_moth_flame.opacity;
    let mut verts = Vec::new();

    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;

    for m in 0..moth_count {
        let phase = m as f32 * std::f32::consts::TAU / moth_count as f32;
        let orbit_angle = now * orbit + phase;
        let spiral = (now * orbit * 0.3 + m as f32).sin() * 10.0 + 20.0;
        let mx = cx + orbit_angle.cos() * spiral;
        let my = cy + orbit_angle.sin() * spiral * 0.7;

        // Moth body
        let c = Color::new(mr, mg, mb, alpha);
        push_rect(&mut verts, mx - 1.5, my - 1.0, 3.0, 2.0, &c);

        // Wings (flapping)
        let wing_angle = (now * orbit * 8.0 + m as f32 * 2.0).sin() * 0.5;
        let wing_w = 4.0 + wing_angle.abs() * 3.0;
        let wing_h = 2.0;
        let wc = Color::new(mr, mg, mb, alpha * 0.5);
        push_rect(
            &mut verts,
            mx - wing_w - 1.0,
            my - wing_h / 2.0,
            wing_w,
            wing_h,
            &wc,
        );
        push_rect(&mut verts, mx + 2.0, my - wing_h / 2.0, wing_w, wing_h, &wc);
    }
    verts
}

/// Emit cursor sparkler effect vertices.
pub(super) fn emit_cursor_sparkler(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_sparkler.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let (sr, sg, sb) = ctx.effects.cursor_sparkler.color;
    let spark_count = ctx.effects.cursor_sparkler.spark_count;
    let burn = ctx.effects.cursor_sparkler.burn_speed;
    let alpha = ctx.effects.cursor_sparkler.opacity;
    let mut verts = Vec::new();

    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;

    for i in 0..spark_count {
        let angle = (i as f32 / spark_count as f32) * std::f32::consts::TAU + now * burn * 2.0;
        let life = (now * burn * 3.0 + i as f32 * 1.7) % 1.0;
        let r = 5.0 + life * 25.0;
        let sx = cx + angle.cos() * r;
        let sy = cy + angle.sin() * r;
        let fade = (1.0 - life).max(0.0);
        let size = 2.0 * fade + 0.5;

        // Color shifts from white-hot to orange to dim
        let heat = fade;
        let cr = sr * heat + 1.0 * (1.0 - heat);
        let cg = sg * heat;
        let cb = sb * heat * 0.3;
        let c = Color::new(cr.min(1.0), cg.min(1.0), cb.min(1.0), alpha * fade);
        push_rect(&mut verts, sx - size / 2.0, sy - size / 2.0, size, size, &c);
    }
    verts
}

/// Emit cursor plasma ball effect vertices.
pub(super) fn emit_cursor_plasma_ball(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_plasma_ball.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let (pr, pg, pb) = ctx.effects.cursor_plasma_ball.color;
    let tendril_count = ctx.effects.cursor_plasma_ball.tendril_count;
    let arc_speed = ctx.effects.cursor_plasma_ball.arc_speed;
    let alpha = ctx.effects.cursor_plasma_ball.opacity;
    let mut verts = Vec::new();

    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;

    for t in 0..tendril_count {
        let base_angle = (t as f32 / tendril_count as f32) * std::f32::consts::TAU;
        let segments = 15;

        for s in 0..segments {
            let frac = s as f32 / segments as f32;
            let r = 3.0 + frac * 25.0;
            let wobble = (now * arc_speed * 5.0 + t as f32 * 2.3 + frac * 4.0).sin() * 0.4;
            let angle = base_angle + now * arc_speed + wobble;
            let px = cx + angle.cos() * r;
            let py = cy + angle.sin() * r;
            let fade = 1.0 - frac * 0.7;
            let size = 2.5 - frac * 1.0;

            let c = Color::new(pr, pg, pb, alpha * fade);
            if size > 0.5 {
                push_rect(&mut verts, px - size / 2.0, py - size / 2.0, size, size, &c);
            }
        }
    }

    // Central glow
    let glow = (now * arc_speed * 3.0).sin() * 0.3 + 0.7;
    let c = Color::new(pr, pg, pb, alpha * glow * 0.5);
    push_rect(&mut verts, cx - 4.0, cy - 4.0, 8.0, 8.0, &c);
    verts
}

/// Emit cursor quill pen effect vertices.
pub(super) fn emit_cursor_quill_pen(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_quill_pen.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let (qr, qg, qb) = ctx.effects.cursor_quill_pen.color;
    let trail_len = ctx.effects.cursor_quill_pen.trail_length;
    let ink_speed = ctx.effects.cursor_quill_pen.ink_speed;
    let alpha = ctx.effects.cursor_quill_pen.opacity;
    let mut verts = Vec::new();

    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;

    // Ink drips falling from cursor
    for i in 0..trail_len {
        let phase = now * ink_speed + i as f32 * 0.7;
        let drip_y = cy + (i as f32 * 8.0) + (phase % 3.0) * 10.0;
        let drip_x = cx + (phase * 1.3).sin() * 4.0;
        let fade = 1.0 - (i as f32 / trail_len as f32);
        let size = 3.0 - i as f32 * 0.2;
        if size > 0.5 {
            let c = Color::new(qr, qg, qb, alpha * fade);
            push_rect(
                &mut verts,
                drip_x - size / 2.0,
                drip_y,
                size,
                size * 1.5,
                &c,
            );
        }
    }

    // Quill flourish curves
    for f in 0..3 {
        let angle_off = f as f32 * std::f32::consts::TAU / 3.0 + now * ink_speed * 0.5;
        for j in 0..8 {
            let t = j as f32 / 8.0;
            let r = 10.0 + t * 15.0;
            let a = angle_off + t * 1.5;
            let fx = cx + a.cos() * r;
            let fy = cy - 5.0 + a.sin() * r * 0.5;
            let fade = 1.0 - t;
            let c = Color::new(qr, qg, qb, alpha * fade * 0.6);
            push_rect(&mut verts, fx - 1.0, fy - 1.0, 2.0, 2.0, &c);
        }
    }
    verts
}

/// Emit cursor aurora borealis effect vertices.
pub(super) fn emit_cursor_aurora_borealis(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_aurora_borealis.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let (ar, ag, ab) = ctx.effects.cursor_aurora_borealis.color;
    let band_count = ctx.effects.cursor_aurora_borealis.band_count;
    let shimmer = ctx.effects.cursor_aurora_borealis.shimmer_speed;
    let alpha = ctx.effects.cursor_aurora_borealis.opacity;
    let mut verts = Vec::new();

    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y;

    // Shimmering bands above cursor
    for band in 0..band_count {
        let band_y = cy - 10.0 - band as f32 * 12.0;
        let band_width = 40.0 + band as f32 * 15.0;
        let segments = 20;

        for s in 0..segments {
            let t = s as f32 / segments as f32;
            let x = cx - band_width / 2.0 + t * band_width;
            let wave =
                (t * std::f32::consts::TAU * 2.0 + now * shimmer * 3.0 + band as f32 * 0.8).sin();
            let y = band_y + wave * 4.0;
            let brightness = (t * std::f32::consts::PI).sin();
            let hue_shift = (band as f32 * 0.3 + now * shimmer * 0.5).sin() * 0.3;

            let cr = (ar + hue_shift).clamp(0.0, 1.0);
            let cg = (ag + hue_shift * 0.5).clamp(0.0, 1.0);
            let cb = (ab - hue_shift * 0.3).clamp(0.0, 1.0);

            let h = 3.0 + wave.abs() * 2.0;
            let fade = 1.0 - (band as f32 / band_count as f32) * 0.5;
            let c = Color::new(cr, cg, cb, alpha * brightness * fade);
            push_rect(&mut verts, x, y, band_width / segments as f32, h, &c);
        }
    }
    verts
}

/// Emit cursor feather effect vertices.
pub(super) fn emit_cursor_feather(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_feather.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;
    let (fr, fg, fb) = ctx.effects.cursor_feather.color;
    let count = ctx.effects.cursor_feather.count;
    let drift = ctx.effects.cursor_feather.drift_speed;
    let opacity = ctx.effects.cursor_feather.opacity;
    let mut verts = Vec::new();
    for i in 0..count {
        let phase = i as f32 * std::f32::consts::TAU / count as f32;
        let t = (now * drift + phase).rem_euclid(3.0);
        let drift_x = (now * 0.5 + phase * 2.0).sin() * 15.0;
        let drift_y = t * 20.0;
        let fx = cx + drift_x;
        let fy = cy - drift_y;
        let alpha = opacity * (1.0 - t / 3.0);
        let sway = (now * 2.0 + phase).sin() * 0.3;
        // Feather quill (center line)
        for s in 0..8 {
            let st = s as f32 / 8.0;
            let qx = fx + st * 6.0 * (1.0 + sway);
            let qy = fy + st * 3.0;
            let c = Color::new(fr, fg, fb, alpha * (1.0 - st * 0.3));
            push_rect(&mut verts, qx, qy, 1.0, 1.0, &c);
        }
        // Feather barbs (side wisps)
        for s in 0..5 {
            let st = s as f32 / 5.0;
            let bx = fx + st * 5.0;
            let by_up = fy + st * 2.0 - 2.0;
            let by_down = fy + st * 2.0 + 2.0;
            let bc = Color::new(fr, fg, fb, alpha * 0.6);
            push_rect(&mut verts, bx, by_up, 2.0, 1.0, &bc);
            push_rect(&mut verts, bx, by_down, 2.0, 1.0, &bc);
        }
    }
    verts
}

/// Emit cursor stardust effect vertices.
pub(super) fn emit_cursor_stardust(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_stardust.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;
    let (sr, sg, sb) = ctx.effects.cursor_stardust.color;
    let count = ctx.effects.cursor_stardust.particle_count;
    let fall = ctx.effects.cursor_stardust.fall_speed;
    let opacity = ctx.effects.cursor_stardust.opacity;
    let mut verts = Vec::new();
    for i in 0..count {
        let phase = i as f32 * std::f32::consts::E;
        let t = (now * fall + phase).rem_euclid(2.0);
        let spread = (phase * 1.618).sin() * 15.0;
        let x = cx + spread + (now * 0.3 + phase).sin() * 3.0;
        let y = cy + t * 25.0;
        let alpha = opacity * (1.0 - t / 2.0);
        let twinkle = (now * 5.0 + phase * 3.0).sin() * 0.5 + 0.5;
        let size = 1.0 + twinkle * 2.0;
        let c = Color::new(sr, sg * (0.8 + twinkle * 0.2), sb, alpha * twinkle);
        push_rect(&mut verts, x - size / 2.0, y - size / 2.0, size, size, &c);
    }
    verts
}

/// Emit cursor compass needle effect vertices.
pub(super) fn emit_cursor_compass_needle(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_compass_needle.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;
    let (nr, ng, nb) = ctx.effects.cursor_compass_needle.color;
    let length = ctx.effects.cursor_compass_needle.length;
    let spin = ctx.effects.cursor_compass_needle.spin_speed;
    let opacity = ctx.effects.cursor_compass_needle.opacity;
    let angle = now * spin;
    let mut verts = Vec::new();
    // Needle segments (north half - colored)
    for s in 0..10 {
        let t = s as f32 / 10.0;
        let x = cx + angle.cos() * length * t;
        let y = cy + angle.sin() * length * t;
        let w = 3.0 * (1.0 - t);
        let c = Color::new(nr, ng, nb, opacity * (1.0 - t * 0.5));
        push_rect(&mut verts, x - w / 2.0, y - w / 2.0, w, w, &c);
    }
    // South half - dimmer
    for s in 0..10 {
        let t = s as f32 / 10.0;
        let x = cx - angle.cos() * length * t;
        let y = cy - angle.sin() * length * t;
        let w = 3.0 * (1.0 - t);
        let c = Color::new(nr * 0.3, ng * 0.3, nb * 0.3, opacity * 0.5 * (1.0 - t));
        push_rect(&mut verts, x - w / 2.0, y - w / 2.0, w, w, &c);
    }
    // Center pivot
    let pc = Color::new(nr, ng, nb, opacity);
    push_rect(&mut verts, cx - 2.0, cy - 2.0, 4.0, 4.0, &pc);
    verts
}

/// Emit cursor galaxy effect vertices.
pub(super) fn emit_cursor_galaxy(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_galaxy.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;
    let (gr, gg, gb) = ctx.effects.cursor_galaxy.color;
    let star_count = ctx.effects.cursor_galaxy.star_count;
    let radius = ctx.effects.cursor_galaxy.radius;
    let opacity = ctx.effects.cursor_galaxy.opacity;
    let mut verts = Vec::new();
    for i in 0..star_count {
        let phase = i as f32 * std::f32::consts::TAU / star_count as f32;
        // Spiral arm pattern
        let arm_angle = now * 0.8 + phase;
        let dist = (i as f32 / star_count as f32) * radius;
        let spiral = arm_angle + dist * 0.1;
        let x = cx + spiral.cos() * dist;
        let y = cy + spiral.sin() * dist;
        let brightness = 1.0 - (dist / radius) * 0.5;
        let twinkle = ((now * 3.0 + phase * 7.0).sin() * 0.5 + 0.5) * 0.4 + 0.6;
        let size = 1.0 + (1.0 - dist / radius) * 2.0;
        let c = Color::new(gr * brightness, gg * brightness, gb, opacity * twinkle);
        push_rect(&mut verts, x - size / 2.0, y - size / 2.0, size, size, &c);
    }
    verts
}

/// Emit cursor prism effect vertices.
pub(super) fn emit_cursor_prism(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_prism.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;
    let ray_count = ctx.effects.cursor_prism.ray_count;
    let spread = ctx.effects.cursor_prism.spread;
    let opacity = ctx.effects.cursor_prism.opacity;
    let mut verts = Vec::new();
    let rainbow: [(f32, f32, f32); 7] = [
        (1.0, 0.0, 0.0),
        (1.0, 0.5, 0.0),
        (1.0, 1.0, 0.0),
        (0.0, 1.0, 0.0),
        (0.0, 0.5, 1.0),
        (0.3, 0.0, 1.0),
        (0.5, 0.0, 0.5),
    ];
    for i in 0..ray_count {
        let base_angle = now * 0.3 + i as f32 * std::f32::consts::TAU / ray_count as f32;
        let (rr, rg, rb) = rainbow[i as usize % 7];
        for seg in 0..20 {
            let dist = seg as f32 * spread * 0.5 + 5.0;
            let x = cx + base_angle.cos() * dist;
            let y = cy + base_angle.sin() * dist;
            let alpha = opacity * (1.0 - seg as f32 / 20.0);
            let size = 2.0 + seg as f32 * 0.3;
            let c = Color::new(rr, rg, rb, alpha);
            push_rect(&mut verts, x - size / 2.0, y - size / 2.0, size, size, &c);
        }
    }
    verts
}

/// Emit cursor moth effect vertices.
pub(super) fn emit_cursor_moth(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_moth.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;
    let moth_count = ctx.effects.cursor_moth.count;
    let wing_size = ctx.effects.cursor_moth.wing_size;
    let (mr, mg, mb) = ctx.effects.cursor_moth.color;
    let opacity = ctx.effects.cursor_moth.opacity;
    let mut verts = Vec::new();
    for i in 0..moth_count {
        let phase = i as f32 * std::f32::consts::TAU / moth_count as f32;
        let orbit_r = 15.0 + 10.0 * (now * 0.7 + phase).sin();
        let orbit_angle = now * 1.5 + phase;
        let mx = cx + orbit_angle.cos() * orbit_r;
        let my = cy + orbit_angle.sin() * orbit_r;
        // Wing flap
        let flap = (now * 8.0 + phase * 3.0).sin().abs();
        let wing_w = wing_size * flap;
        let wing_h = wing_size * 0.6;
        // Left wing
        let c = Color::new(mr, mg, mb, opacity * flap);
        push_rect(
            &mut verts,
            mx - wing_w,
            my - wing_h / 2.0,
            wing_w,
            wing_h,
            &c,
        );
        // Right wing
        push_rect(&mut verts, mx, my - wing_h / 2.0, wing_w, wing_h, &c);
        // Body
        let bc = Color::new(mr * 0.7, mg * 0.7, mb * 0.7, opacity);
        push_rect(
            &mut verts,
            mx - 1.0,
            my - wing_h * 0.4,
            2.0,
            wing_h * 0.8,
            &bc,
        );
    }
    verts
}

/// Emit cursor sparkle burst effect vertices.
/// Returns (vertices, needs_continuous_redraw).
/// `entries` is the persistent state for sparkle burst tracking.
pub(super) fn emit_cursor_sparkle_burst(
    ctx: &EffectCtx,
    entries: &mut Vec<SparkleBurstEntry>,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.cursor_sparkle_burst.enabled || !ctx.cursor_visible {
        return (Vec::new(), false);
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return (Vec::new(), false),
    };
    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;
    // Detect cursor movement to spawn burst
    let last = entries.last();
    let should_spawn = match last {
        Some(e) => (cx - e.cx).abs() > 2.0 || (cy - e.cy).abs() > 2.0,
        None => true,
    };
    if should_spawn {
        let seed = (cx as u32)
            .wrapping_mul(31)
            .wrapping_add(cy as u32)
            .wrapping_mul(17)
            // RNG SEED, NOT A TIME: the clock read mixed in here was entropy.
            // The frame's sample is frame-constant, so it would make every
            // burst spawned at one position identical forever; the frame
            // counter varies it across frames while position varies it within.
            .wrapping_add(effect_entity_seed(ctx.frame_seq, entries.len() as u64) as u32);
        entries.push(SparkleBurstEntry {
            cx,
            cy,
            started: ctx.frame_sample.presentation_time(),
            seed,
        });
        if entries.len() > 20 {
            entries.remove(0);
        }
    }
    let (sr, sg, sb) = ctx.effects.cursor_sparkle_burst.color;
    let sop = ctx.effects.cursor_sparkle_burst.opacity;
    let count = ctx.effects.cursor_sparkle_burst.count;
    let radius = ctx.effects.cursor_sparkle_burst.radius;
    let mut verts: Vec<RectVertex> = Vec::new();
    let duration = 0.4_f32;
    entries.retain(|e| {
        ctx.frame_sample
            .since_at_presentation(e.started)
            .as_secs_f32()
            < duration
    });
    for entry in entries.iter() {
        let t = ctx
            .frame_sample
            .since_at_presentation(entry.started)
            .as_secs_f32()
            / duration;
        let fade = 1.0 - t;
        for i in 0..count {
            let mut h = entry.seed.wrapping_add(i * 2654435761);
            h ^= h >> 16;
            h = h.wrapping_mul(0x45d9f3b);
            let angle = (h as f32 / u32::MAX as f32) * std::f32::consts::PI * 2.0;
            h = h.wrapping_mul(0x119de1f3);
            let speed_var = 0.5 + (h as f32 / u32::MAX as f32) * 0.5;
            let r = t * radius * speed_var;
            let px = entry.cx + angle.cos() * r;
            let py = entry.cy + angle.sin() * r;
            let sz = 2.0 * fade;
            let c = Color::new(sr, sg, sb, sop * fade * fade);
            push_rect(&mut verts, px - sz / 2.0, py - sz / 2.0, sz, sz, &c);
        }
    }
    let needs_redraw = !verts.is_empty();
    (verts, needs_redraw)
}

/// Emit cursor compass rose effect vertices.
pub(super) fn emit_cursor_compass_rose(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_compass.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;
    let (cr, cg, cb) = ctx.effects.cursor_compass.color;
    let cop = ctx.effects.cursor_compass.opacity;
    let size = ctx.effects.cursor_compass.size;
    let spd = ctx.effects.cursor_compass.speed;
    let rotation = now * spd;
    let mut verts: Vec<RectVertex> = Vec::new();
    // 4 cardinal lines + 4 intermediate lines
    for i in 0..8 {
        let angle = rotation + i as f32 / 8.0 * std::f32::consts::PI * 2.0;
        let is_cardinal = i % 2 == 0;
        let len = if is_cardinal { size } else { size * 0.6 };
        let thick = if is_cardinal { 2.0 } else { 1.0 };
        let alpha = if is_cardinal { cop } else { cop * 0.6 };
        let seg_count = 8;
        for seg in 0..seg_count {
            let t = seg as f32 / seg_count as f32;
            let r = t * len;
            let px = cx + angle.cos() * r;
            let py = cy + angle.sin() * r;
            let fade = 1.0 - t * 0.3;
            let c = Color::new(cr, cg, cb, alpha * fade);
            push_rect(
                &mut verts,
                px - thick / 2.0,
                py - thick / 2.0,
                thick,
                thick,
                &c,
            );
        }
        // Arrow tip for cardinal directions
        if is_cardinal {
            let tip_r = len * 0.9;
            let tip_angle1 = angle + 0.15;
            let tip_angle2 = angle - 0.15;
            let tx1 = cx + tip_angle1.cos() * tip_r;
            let ty1 = cy + tip_angle1.sin() * tip_r;
            let tx2 = cx + tip_angle2.cos() * tip_r;
            let ty2 = cy + tip_angle2.sin() * tip_r;
            let tc = Color::new(cr, cg, cb, alpha * 0.8);
            push_rect(&mut verts, tx1 - 1.5, ty1 - 1.5, 3.0, 3.0, &tc);
            push_rect(&mut verts, tx2 - 1.5, ty2 - 1.5, 3.0, 3.0, &tc);
        }
    }
    // Center dot
    let center_c = Color::new(cr, cg, cb, cop);
    push_rect(&mut verts, cx - 2.0, cy - 2.0, 4.0, 4.0, &center_c);
    verts
}

/// Emit cursor DNA helix trail effect vertices.
pub(super) fn emit_cursor_dna_helix(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_dna_helix.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let now = ctx
        .frame_sample
        .since_at_presentation(ctx.aurora_start)
        .as_secs_f32();
    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;
    let (c1r, c1g, c1b) = ctx.effects.cursor_dna_helix.color1;
    let (c2r, c2g, c2b) = ctx.effects.cursor_dna_helix.color2;
    let dop = ctx.effects.cursor_dna_helix.opacity;
    let radius = ctx.effects.cursor_dna_helix.radius;
    let spd = ctx.effects.cursor_dna_helix.speed;
    let mut verts: Vec<RectVertex> = Vec::new();
    let num_nodes = 16;
    let dot_size = 3.0;
    for i in 0..num_nodes {
        let t = i as f32 / num_nodes as f32;
        let angle = now * spd * 3.0 + t * std::f32::consts::PI * 4.0;
        let vert_offset = (t - 0.5) * anim.height * 2.0;
        let fade = 1.0 - t;
        // Strand 1
        let x1 = cx + angle.cos() * radius;
        let y1 = cy + vert_offset;
        let c1 = Color::new(c1r, c1g, c1b, dop * fade);
        push_rect(
            &mut verts,
            x1 - dot_size / 2.0,
            y1 - dot_size / 2.0,
            dot_size,
            dot_size,
            &c1,
        );
        // Strand 2 (opposite phase)
        let x2 = cx - angle.cos() * radius;
        let y2 = cy + vert_offset;
        let c2 = Color::new(c2r, c2g, c2b, dop * fade);
        push_rect(
            &mut verts,
            x2 - dot_size / 2.0,
            y2 - dot_size / 2.0,
            dot_size,
            dot_size,
            &c2,
        );
        // Cross-link every 4th node
        if i % 4 == 0 {
            let link_c = Color::new(
                (c1r + c2r) * 0.5,
                (c1g + c2g) * 0.5,
                (c1b + c2b) * 0.5,
                dop * fade * 0.5,
            );
            let lx = x1.min(x2);
            let lw = (x2 - x1).abs().max(1.0);
            push_rect(&mut verts, lx, y1 - 0.5, lw, 1.0, &link_c);
        }
    }
    verts
}

/// Emit cursor pendulum swing effect vertices.
/// Returns (vertices, needs_continuous_redraw).
/// Mutable state params track the pendulum's last position and swing start time.
pub(super) fn emit_cursor_pendulum(
    ctx: &EffectCtx,
    last_x: &mut f32,
    last_y: &mut f32,
    swing_start: &mut Option<EventTime>,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.cursor_pendulum.enabled || !ctx.cursor_visible {
        return (Vec::new(), false);
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return (Vec::new(), false),
    };
    let cx = anim.x + anim.width / 2.0;
    let cy = anim.y + anim.height / 2.0;
    // Detect cursor move
    if (cx - *last_x).abs() > 1.0 || (cy - *last_y).abs() > 1.0 {
        *swing_start = Some(ctx.frame_sample.presentation_time());
        *last_x = cx;
        *last_y = cy;
    }
    let start = match swing_start {
        Some(s) => *s,
        None => return (Vec::new(), false),
    };
    let elapsed = ctx.frame_sample.since_at_presentation(start).as_secs_f32();
    let (pr, pg, pb) = ctx.effects.cursor_pendulum.color;
    let pop = ctx.effects.cursor_pendulum.opacity;
    let arc_len = ctx.effects.cursor_pendulum.arc_length;
    let damping = ctx.effects.cursor_pendulum.damping;
    let decay = (-elapsed * damping * 5.0).exp();
    if decay <= 0.01 {
        *swing_start = None;
        return (Vec::new(), false);
    }
    let swing_angle = (elapsed * 8.0).sin() * decay * std::f32::consts::PI * 0.4;
    let mut verts: Vec<RectVertex> = Vec::new();
    let num_seg = 12;
    for seg in 0..num_seg {
        let t = seg as f32 / num_seg as f32;
        let r = t * arc_len;
        let angle = swing_angle + std::f32::consts::PI * 0.5;
        let px = cx + angle.cos() * r;
        let py = cy + angle.sin() * r;
        let fade = (1.0 - t) * decay;
        let dot = 2.0 + t * 2.0;
        let c = Color::new(pr, pg, pb, pop * fade);
        push_rect(&mut verts, px - dot / 2.0, py - dot / 2.0, dot, dot, &c);
    }
    // Bob at end
    let bob_r = arc_len;
    let bob_angle = swing_angle + std::f32::consts::PI * 0.5;
    let bx = cx + bob_angle.cos() * bob_r;
    let by = cy + bob_angle.sin() * bob_r;
    let bob_size = 6.0;
    let bc = Color::new(pr, pg, pb, pop * decay);
    push_rect(
        &mut verts,
        bx - bob_size / 2.0,
        by - bob_size / 2.0,
        bob_size,
        bob_size,
        &bc,
    );
    let needs_redraw = !verts.is_empty();
    (verts, needs_redraw)
}

/// Emit cursor drop shadow vertices.
pub(super) fn emit_cursor_drop_shadow(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.cursor_shadow.enabled || !ctx.cursor_visible {
        return Vec::new();
    }
    let anim = match ctx.animated_cursor {
        Some(anim) => anim,
        None => return Vec::new(),
    };
    let sx = anim.x + ctx.effects.cursor_shadow.offset_x;
    let sy = anim.y + ctx.effects.cursor_shadow.offset_y;
    let shadow_alpha = ctx.effects.cursor_shadow.opacity.clamp(0.0, 1.0);
    let shadow_c = Color::new(0.0, 0.0, 0.0, shadow_alpha);
    let mut verts: Vec<RectVertex> = Vec::new();
    push_rect(&mut verts, sx, sy, anim.width, anim.height, &shadow_c);
    verts
}

/// Emit cursor trail fade (afterimage ghost) effect vertices.
/// Returns (vertices, needs_continuous_redraw).
/// `positions` is the persistent trail state: Vec of (x, y, w, h, spawn_time).
/// `fade_duration` controls how long each afterimage lasts.
pub(super) fn emit_cursor_trail_fade(
    ctx: &EffectCtx,
    positions: &mut Vec<(f32, f32, f32, f32, EventTime)>,
    fade_duration: &std::time::Duration,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.cursor_trail_fade.enabled || positions.is_empty() {
        return (Vec::new(), false);
    }
    let now = ctx.frame_sample.presentation_time();
    let fade_dur = *fade_duration;

    // Remove expired positions
    positions.retain(|&(_, _, _, _, t)| now.saturating_since(t) < fade_dur);

    let mut verts: Vec<RectVertex> = Vec::new();
    for &(tx, ty, tw, th, spawn) in positions.iter() {
        let elapsed = now.saturating_since(spawn).as_secs_f32();
        let t = (elapsed / fade_dur.as_secs_f32()).min(1.0);
        let alpha = 0.3 * (1.0 - t) * (1.0 - t);
        if alpha < 0.005 {
            continue;
        }
        let c = Color::new(0.5, 0.7, 1.0, alpha);
        push_rect(&mut verts, tx, ty, tw, th, &c);
    }
    let needs_redraw = !verts.is_empty();
    (verts, needs_redraw)
}

#[cfg(test)]
#[path = "cursor_effects_test.rs"]
mod tests;
