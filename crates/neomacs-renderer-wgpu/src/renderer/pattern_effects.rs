//! Pattern and overlay visual effects.
//!
//! Contains background patterns, geometric overlays, and edge effects
//! extracted from the main render_frame_glyphs function.

use super::super::vertex::RectVertex;
use super::effect_common::{EffectCtx, push_rect};
use neomacs_display_protocol::types::Color;

// ============================================================================
// Background pattern (dots/grid/crosshatch)
// ============================================================================

pub(super) fn emit_background_pattern(ctx: &EffectCtx) -> Vec<RectVertex> {
    if ctx.effects.bg_pattern.style == 0 {
        return Vec::new();
    }
    let spacing = ctx.effects.bg_pattern.spacing.max(4.0);
    let (pr, pg, pb) = ctx.effects.bg_pattern.color;
    let alpha = ctx.effects.bg_pattern.opacity;
    let pat_color = Color::new(pr, pg, pb, alpha);
    let frame_w = ctx.frame_glyphs.width;
    let frame_h = ctx.frame_glyphs.height;
    let mut verts: Vec<RectVertex> = Vec::new();

    match ctx.effects.bg_pattern.style {
        1 => {
            // Dots: small squares at grid intersections
            let dot_size = 1.0;
            let mut y = 0.0_f32;
            while y < frame_h {
                let mut x = 0.0_f32;
                while x < frame_w {
                    push_rect(&mut verts, x, y, dot_size, dot_size, &pat_color);
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
                push_rect(&mut verts, 0.0, y, frame_w, line_w, &pat_color);
                y += spacing;
            }
            let mut x = 0.0_f32;
            while x < frame_w {
                push_rect(&mut verts, x, 0.0, line_w, frame_h, &pat_color);
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
                        push_rect(&mut verts, px, py, line_w, step, &pat_color);
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
                        push_rect(&mut verts, px, py, line_w, step, &pat_color);
                    }
                    t += step;
                }
                offset += diag_spacing;
            }
        }
        _ => {}
    }

    verts
}

// ============================================================================
// Heat distortion effect
// ============================================================================

pub(super) fn emit_heat_distortion(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.heat_distortion.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let ew = ctx.effects.heat_distortion.edge_width;
    let intensity = ctx.effects.heat_distortion.intensity;
    let spd = ctx.effects.heat_distortion.speed;
    let op = ctx.effects.heat_distortion.opacity;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    let strip_count = 12;
    for i in 0..strip_count {
        let phase = i as f32 / strip_count as f32 * std::f32::consts::PI * 2.0;
        let wave = ((now * spd * 3.0 + phase).sin() * 0.5 + 0.5) * intensity;
        let alpha = op * wave;
        let cr = 1.0;
        let cg = 0.6 + wave * 0.4;
        let cb = 0.2;
        let ty = i as f32 * ew / strip_count as f32;
        let fade = 1.0 - ty / ew;
        let sh = ew / strip_count as f32;
        let c = Color::new(cr, cg, cb, alpha * fade);
        push_rect(&mut verts, 0.0, ty, fw, sh, &c);
        let by = fh - ew + ty;
        push_rect(&mut verts, 0.0, by, fw, sh, &c);
        let lx = i as f32 * ew / strip_count as f32;
        push_rect(&mut verts, lx, 0.0, sh, fh, &c);
        let rx = fw - ew + lx;
        push_rect(&mut verts, rx, 0.0, sh, fh, &c);
    }
    verts
}

// ============================================================================
// Neon border effect
// ============================================================================

pub(super) fn emit_neon_border(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.neon_border.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (nr, ng, nb) = ctx.effects.neon_border.color;
    let thick = ctx.effects.neon_border.thickness;
    let intensity = ctx.effects.neon_border.intensity;
    let flicker = ctx.effects.neon_border.flicker;
    let nop = ctx.effects.neon_border.opacity;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let flicker_val = if flicker > 0.0 {
        let f1 = (now * 7.3).sin() * 0.5 + 0.5;
        let f2 = (now * 13.7).sin() * 0.5 + 0.5;
        1.0 - flicker * f1 * f2
    } else {
        1.0
    };
    let final_op = nop * intensity * flicker_val;
    let glow_layers = 4;
    let mut verts: Vec<RectVertex> = Vec::new();
    for layer in 0..glow_layers {
        let expand = layer as f32 * thick * 0.8;
        let layer_alpha = final_op / (1.0 + layer as f32 * 1.5);
        let t = thick + expand;
        let c = Color::new(nr, ng, nb, layer_alpha);
        push_rect(&mut verts, 0.0, 0.0, fw, t, &c);
        push_rect(&mut verts, 0.0, fh - t, fw, t, &c);
        push_rect(&mut verts, 0.0, t, t, fh - 2.0 * t, &c);
        push_rect(&mut verts, fw - t, t, t, fh - 2.0 * t, &c);
    }
    verts
}

// ============================================================================
// Plasma border effect
// ============================================================================

pub(super) fn emit_plasma_border(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.plasma_border.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (r1, g1, b1) = ctx.effects.plasma_border.color1;
    let (r2, g2, b2) = ctx.effects.plasma_border.color2;
    let bw = ctx.effects.plasma_border.width;
    let spd = ctx.effects.plasma_border.speed;
    let pop = ctx.effects.plasma_border.opacity;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let perimeter = 2.0 * (fw + fh);
    let seg_count = 80u32;
    let seg_len = perimeter / seg_count as f32;
    let mut verts: Vec<RectVertex> = Vec::new();
    for i in 0..seg_count {
        let t = i as f32 / seg_count as f32;
        let dist = t * perimeter;
        let phase = now * spd * 3.0 + t * 12.0;
        let blend = phase.sin() * 0.5 + 0.5;
        let cr = r1 * (1.0 - blend) + r2 * blend;
        let cg = g1 * (1.0 - blend) + g2 * blend;
        let cb = b1 * (1.0 - blend) + b2 * blend;
        let pulse = 0.7 + 0.3 * (phase * 0.7).sin();
        let c = Color::new(cr, cg, cb, pop * pulse);
        // Map distance to position on border
        if dist < fw {
            // Top edge
            push_rect(&mut verts, dist, 0.0, seg_len.min(fw - dist), bw, &c);
        } else if dist < fw + fh {
            // Right edge
            let d = dist - fw;
            push_rect(&mut verts, fw - bw, d, bw, seg_len.min(fh - d), &c);
        } else if dist < 2.0 * fw + fh {
            // Bottom edge
            let d = dist - fw - fh;
            push_rect(
                &mut verts,
                fw - d - seg_len.min(fw - d),
                fh - bw,
                seg_len.min(fw - d),
                bw,
                &c,
            );
        } else {
            // Left edge
            let d = dist - 2.0 * fw - fh;
            push_rect(
                &mut verts,
                0.0,
                fh - d - seg_len.min(fh - d),
                bw,
                seg_len.min(fh - d),
                &c,
            );
        }
    }
    verts
}

// ============================================================================
// Topographic contour effect
// ============================================================================

pub(super) fn emit_topographic_contour(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.topo_contour.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (tr, tg, tb) = ctx.effects.topo_contour.color;
    let top = ctx.effects.topo_contour.opacity;
    let spacing = ctx.effects.topo_contour.spacing.max(5.0);
    let spd = ctx.effects.topo_contour.speed;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    let num_contours = (fh / spacing) as i32 + 2;
    let line_thick = 1.0;
    for i in 0..num_contours {
        let base_y = i as f32 * spacing + (now * spd * 10.0) % spacing;
        let num_seg = 40;
        for seg in 0..num_seg {
            let x = seg as f32 / num_seg as f32 * fw;
            let wave = (x * 0.01 + i as f32 * 0.5 + now * spd * 0.3).sin() * spacing * 0.3
                + (x * 0.02 + i as f32 * 1.2).cos() * spacing * 0.15;
            let y = base_y + wave;
            if y >= 0.0 && y < fh {
                let seg_w = fw / num_seg as f32 + 1.0;
                let alpha = top * (0.5 + 0.5 * (x * 0.005 + now * 0.5).sin());
                let c = Color::new(tr, tg, tb, alpha);
                push_rect(&mut verts, x, y, seg_w, line_thick, &c);
            }
        }
    }
    verts
}

// ============================================================================
// Constellation overlay effect
// ============================================================================

pub(super) fn emit_constellation(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.constellation.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (sr, sg, sb) = ctx.effects.constellation.color;
    let sop = ctx.effects.constellation.opacity;
    let count = ctx.effects.constellation.star_count.min(200);
    let conn_dist = ctx.effects.constellation.connect_dist;
    let tspd = ctx.effects.constellation.twinkle_speed;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    // Generate deterministic star positions
    let mut stars: Vec<(f32, f32)> = Vec::with_capacity(count as usize);
    for i in 0..count {
        let mut h = i.wrapping_mul(2654435761);
        h ^= h >> 16;
        let x = (h as f32 / u32::MAX as f32) * fw;
        h = h.wrapping_mul(0x45d9f3b);
        h ^= h >> 16;
        let y = (h as f32 / u32::MAX as f32) * fh;
        stars.push((x, y));
        // Twinkle
        let twinkle = (0.4 + 0.6 * (now * tspd + i as f32 * 2.1).sin()).max(0.0);
        let sz = 2.0 + twinkle * 2.0;
        let c = Color::new(sr, sg, sb, sop * twinkle);
        push_rect(&mut verts, x - sz / 2.0, y - sz / 2.0, sz, sz, &c);
    }
    // Connect nearby stars
    for i in 0..stars.len() {
        for j in (i + 1)..stars.len().min(i + 10) {
            let dx = stars[j].0 - stars[i].0;
            let dy = stars[j].1 - stars[i].1;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < conn_dist && dist > 1.0 {
                let alpha = sop * 0.3 * (1.0 - dist / conn_dist);
                let c = Color::new(sr, sg, sb, alpha);
                let mx = stars[i].0.min(stars[j].0);
                let my = stars[i].1.min(stars[j].1);
                let lw = dx.abs().max(1.0);
                let lh = dy.abs().max(1.0);
                push_rect(&mut verts, mx, my, lw, lh, &c);
            }
        }
    }
    verts
}

// ============================================================================
// Kaleidoscope overlay effect
// ============================================================================

pub(super) fn emit_kaleidoscope(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.kaleidoscope.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (kr, kg, kb) = ctx.effects.kaleidoscope.color;
    let kop = ctx.effects.kaleidoscope.opacity;
    let segs = ctx.effects.kaleidoscope.segments.clamp(3, 12);
    let spd = ctx.effects.kaleidoscope.speed;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let cx = fw / 2.0;
    let cy = fh / 2.0;
    let mut verts: Vec<RectVertex> = Vec::new();
    let angle_step = std::f32::consts::PI * 2.0 / segs as f32;
    let max_r = (fw * fw + fh * fh).sqrt() * 0.5;
    for seg in 0..segs {
        let base_angle = seg as f32 * angle_step + now * spd * 0.3;
        let num_shapes = 8;
        for s in 0..num_shapes {
            let r = max_r * (s as f32 + 1.0) / num_shapes as f32 * 0.8;
            let wobble = (now * spd * 0.7 + s as f32 * 1.3).sin() * 0.15;
            let a = base_angle + wobble;
            let px = cx + a.cos() * r;
            let py = cy + a.sin() * r;
            let sz = 4.0 + (now * spd + s as f32 * 0.7).sin().abs() * 8.0;
            let alpha = kop * (0.3 + 0.7 * (1.0 - r / max_r));
            let c = Color::new(kr, kg, kb, alpha);
            push_rect(&mut verts, px - sz / 2.0, py - sz / 2.0, sz, sz, &c);
        }
    }
    verts
}

// ============================================================================
// Noise field overlay effect
// ============================================================================

pub(super) fn emit_noise_field(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.noise_field.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (nr, ng, nb) = ctx.effects.noise_field.color;
    let nop = ctx.effects.noise_field.opacity;
    let scale = ctx.effects.noise_field.scale.max(10.0);
    let spd = ctx.effects.noise_field.speed;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    let step = scale;
    let cols = (fw / step) as i32 + 1;
    let rows = (fh / step) as i32 + 1;
    for row in 0..rows {
        for col in 0..cols {
            let x = col as f32 * step;
            let y = row as f32 * step;
            // Pseudo-noise using sin combinations
            let n = ((x * 0.013 + now * spd * 0.7).sin()
                * (y * 0.017 + now * spd * 0.5).cos()
                * (x * 0.009 + y * 0.011 + now * spd * 0.3).sin())
            .abs();
            let alpha = nop * n;
            if alpha > 0.005 {
                let c = Color::new(nr, ng, nb, alpha);
                push_rect(&mut verts, x, y, step, step, &c);
            }
        }
    }
    verts
}

// ============================================================================
// Spiral vortex overlay effect
// ============================================================================

pub(super) fn emit_spiral_vortex(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.spiral_vortex.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (vr, vg, vb) = ctx.effects.spiral_vortex.color;
    let vop = ctx.effects.spiral_vortex.opacity;
    let arms = ctx.effects.spiral_vortex.arms.clamp(2, 12);
    let spd = ctx.effects.spiral_vortex.speed;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let cx = fw / 2.0;
    let cy = fh / 2.0;
    let max_r = (fw * fw + fh * fh).sqrt() * 0.5;
    let mut verts: Vec<RectVertex> = Vec::new();
    for arm in 0..arms {
        let arm_offset = arm as f32 * std::f32::consts::PI * 2.0 / arms as f32;
        let steps = 60;
        for step in 0..steps {
            let t = step as f32 / steps as f32;
            let r = t * max_r;
            let angle = arm_offset + t * 6.0 + now * spd;
            let px = cx + angle.cos() * r;
            let py = cy + angle.sin() * r;
            let sz = 2.0 + t * 3.0;
            let alpha = vop * (1.0 - t * 0.7);
            let c = Color::new(vr, vg, vb, alpha);
            push_rect(&mut verts, px - sz / 2.0, py - sz / 2.0, sz, sz, &c);
        }
    }
    verts
}

// ============================================================================
// Diamond lattice overlay effect
// ============================================================================

pub(super) fn emit_diamond_lattice(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.diamond_lattice.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (dr, dg, db) = ctx.effects.diamond_lattice.color;
    let dop = ctx.effects.diamond_lattice.opacity;
    let cell = ctx.effects.diamond_lattice.cell_size.max(10.0);
    let shspd = ctx.effects.diamond_lattice.shimmer_speed;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    let half = cell / 2.0;
    let cols = (fw / cell) as i32 + 2;
    let rows = (fh / half) as i32 + 2;
    let line_thick = 1.0;
    for row in 0..rows {
        for col in 0..cols {
            let cx = col as f32 * cell + if row % 2 == 1 { half } else { 0.0 };
            let cy = row as f32 * half;
            let shimmer = (0.5 + 0.5 * ((cx * 0.02 + cy * 0.02 + now * shspd).sin())).max(0.0);
            let alpha = dop * shimmer;
            let c = Color::new(dr, dg, db, alpha);
            // Draw diamond edges as small rects
            // Top-right edge
            let segs = 4;
            for s in 0..segs {
                let t = s as f32 / segs as f32;
                let px = cx + t * half;
                let py = cy - half + t * half;
                push_rect(&mut verts, px, py, line_thick, line_thick, &c);
            }
            // Bottom-right edge
            for s in 0..segs {
                let t = s as f32 / segs as f32;
                let px = cx + half - t * half;
                let py = cy + t * half;
                push_rect(&mut verts, px, py, line_thick, line_thick, &c);
            }
        }
    }
    verts
}

// ============================================================================
// Wave interference overlay effect
// ============================================================================

pub(super) fn emit_wave_interference(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.wave_interference.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (wr, wg, wb) = ctx.effects.wave_interference.color;
    let wop = ctx.effects.wave_interference.opacity;
    let wl = ctx.effects.wave_interference.wavelength.max(10.0);
    let sources = ctx.effects.wave_interference.source_count.clamp(2, 6);
    let spd = ctx.effects.wave_interference.speed;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    // Generate deterministic source positions
    let mut src_pos: Vec<(f32, f32)> = Vec::new();
    for i in 0..sources {
        let mut h = i.wrapping_mul(2654435761);
        h ^= h >> 16;
        let x = (h as f32 / u32::MAX as f32) * fw;
        h = h.wrapping_mul(0x45d9f3b);
        h ^= h >> 16;
        let y = (h as f32 / u32::MAX as f32) * fh;
        src_pos.push((x, y));
    }
    let step = wl * 0.5;
    let cols = (fw / step) as i32 + 1;
    let rows = (fh / step) as i32 + 1;
    for row in 0..rows {
        for col in 0..cols {
            let x = col as f32 * step;
            let y = row as f32 * step;
            let mut val = 0.0f32;
            for src in &src_pos {
                let dx = x - src.0;
                let dy = y - src.1;
                let dist = (dx * dx + dy * dy).sqrt();
                val += (dist / wl * std::f32::consts::PI * 2.0 - now * spd * 3.0).sin();
            }
            let intensity = (val / sources as f32).abs();
            let alpha = wop * intensity;
            if alpha > 0.005 {
                let c = Color::new(wr, wg, wb, alpha);
                push_rect(&mut verts, x, y, step, step, &c);
            }
        }
    }
    verts
}

// ============================================================================
// Chevron pattern overlay effect
// ============================================================================

pub(super) fn emit_chevron(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.chevron_pattern.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (cr, cg, cb) = ctx.effects.chevron_pattern.color;
    let cop = ctx.effects.chevron_pattern.opacity;
    let spacing = ctx.effects.chevron_pattern.spacing.max(15.0);
    let spd = ctx.effects.chevron_pattern.speed;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    let rows = (fh / spacing) as i32 + 2;
    let line_thick = 1.0;
    for row in 0..rows {
        let base_y = row as f32 * spacing + (now * spd * 20.0) % spacing;
        let num_v = (fw / spacing) as i32 + 1;
        for v in 0..num_v {
            let vx = v as f32 * spacing;
            // Draw V shape: left leg
            let segs = 4;
            for s in 0..segs {
                let t = s as f32 / segs as f32;
                let px = vx + t * spacing * 0.5;
                let py = base_y - t * spacing * 0.5;
                let shimmer = (0.5 + 0.5 * ((vx * 0.01 + now * spd).sin())).max(0.0);
                let c = Color::new(cr, cg, cb, cop * shimmer);
                push_rect(&mut verts, px, py, line_thick * 2.0, line_thick, &c);
            }
            // Right leg
            for s in 0..segs {
                let t = s as f32 / segs as f32;
                let px = vx + spacing * 0.5 + t * spacing * 0.5;
                let py = base_y - spacing * 0.5 + t * spacing * 0.5;
                let shimmer = (0.5 + 0.5 * ((vx * 0.01 + now * spd).sin())).max(0.0);
                let c = Color::new(cr, cg, cb, cop * shimmer);
                push_rect(&mut verts, px, py, line_thick * 2.0, line_thick, &c);
            }
        }
    }
    verts
}

// ============================================================================
// Sunburst pattern overlay effect
// ============================================================================

pub(super) fn emit_sunburst(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.sunburst_pattern.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (cr, cg, cb) = ctx.effects.sunburst_pattern.color;
    let ray_count = ctx.effects.sunburst_pattern.ray_count.max(4) as f32;
    let speed = ctx.effects.sunburst_pattern.speed;
    let opacity = ctx.effects.sunburst_pattern.opacity;
    let mut verts: Vec<RectVertex> = Vec::new();
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let cx = width / 2.0;
    let cy = height / 2.0;
    let max_radius = (cx * cx + cy * cy).sqrt();
    let rotation = now * speed;
    // Draw rays as thin triangular wedges from center
    for i in 0..ctx.effects.sunburst_pattern.ray_count {
        let angle = rotation + (i as f32 / ray_count) * std::f32::consts::TAU;
        let half_width = std::f32::consts::PI / ray_count * 0.4;
        let a1 = angle - half_width;
        let a2 = angle + half_width;
        // Draw ray as series of small rects along the ray direction
        let steps = 30u32;
        for s in 0..steps {
            let t0 = s as f32 / steps as f32;
            let t1 = (s + 1) as f32 / steps as f32;
            let r0 = t0 * max_radius;
            let r1 = t1 * max_radius;
            let mid_angle = (a1 + a2) / 2.0;
            let x0 = cx + mid_angle.cos() * r0;
            let y0 = cy + mid_angle.sin() * r0;
            let x1 = cx + mid_angle.cos() * r1;
            let y1 = cy + mid_angle.sin() * r1;
            let w = (r0 * (a2 - a1)).abs().max(1.0);
            let mx = (x0 + x1) / 2.0;
            let my = (y0 + y1) / 2.0;
            let seg_len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt().max(1.0);
            let fade = 1.0 - t0;
            let alpha = opacity * fade;
            let c = Color::new(cr, cg, cb, alpha);
            push_rect(&mut verts, mx - w / 2.0, my - seg_len / 2.0, w, seg_len, &c);
        }
    }
    verts
}

// ============================================================================
// Honeycomb dissolve overlay effect
// ============================================================================

pub(super) fn emit_honeycomb_dissolve(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.honeycomb_dissolve.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (cr, cg, cb) = ctx.effects.honeycomb_dissolve.color;
    let cell = ctx.effects.honeycomb_dissolve.cell_size.max(8.0);
    let speed = ctx.effects.honeycomb_dissolve.speed;
    let opacity = ctx.effects.honeycomb_dissolve.opacity;
    let mut verts: Vec<RectVertex> = Vec::new();
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    // Hex grid layout
    let hex_h = cell * 0.866; // sqrt(3)/2
    let cols = (width / (cell * 1.5)) as u32 + 2;
    let rows = (height / hex_h) as u32 + 2;
    for row in 0..rows {
        for col in 0..cols {
            let offset_x = if row % 2 == 1 { cell * 0.75 } else { 0.0 };
            let hx = col as f32 * cell * 1.5 + offset_x;
            let hy = row as f32 * hex_h;
            // Deterministic phase per cell
            let mut h = ((row * 137 + col) as u64).wrapping_mul(2654435761);
            h ^= h >> 16;
            let phase = (h % 1000) as f32 / 1000.0 * std::f32::consts::TAU;
            let dissolve = (now * speed + phase).sin() * 0.5 + 0.5;
            if dissolve > 0.1 {
                let alpha = opacity * dissolve;
                let cell_draw = cell * 0.4 * dissolve;
                let c = Color::new(cr, cg, cb, alpha);
                push_rect(
                    &mut verts,
                    hx - cell_draw / 2.0,
                    hy - cell_draw / 2.0,
                    cell_draw,
                    cell_draw,
                    &c,
                );
            }
        }
    }
    verts
}

// ============================================================================
// Moire pattern overlay effect
// ============================================================================

pub(super) fn emit_moire(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.moire_pattern.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (cr, cg, cb) = ctx.effects.moire_pattern.color;
    let spacing = ctx.effects.moire_pattern.line_spacing.max(4.0);
    let angle_off = ctx.effects.moire_pattern.angle_offset * std::f32::consts::PI / 180.0;
    let speed = ctx.effects.moire_pattern.speed;
    let opacity = ctx.effects.moire_pattern.opacity;
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    let diagonal = (width * width + height * height).sqrt();
    // Two sets of parallel lines at different angles
    for layer in 0..2 {
        let base_angle = if layer == 0 {
            now * speed
        } else {
            now * speed + angle_off
        };
        let cos_a = base_angle.cos();
        let sin_a = base_angle.sin();
        let line_count = (diagonal / spacing) as i32 + 2;
        for i in (-line_count)..line_count {
            let offset = i as f32 * spacing;
            // Line perpendicular to angle direction
            let lx = width / 2.0 + cos_a * offset;
            let ly = height / 2.0 + sin_a * offset;
            // Draw as thin rect along the line direction
            let line_w = 1.0;
            let line_h = diagonal;
            // Approximate with axis-aligned rect
            let alpha = opacity * 0.5;
            let c = Color::new(cr, cg, cb, alpha);
            push_rect(
                &mut verts,
                lx - line_w / 2.0,
                ly - line_h / 2.0,
                line_w,
                line_h,
                &c,
            );
        }
    }
    verts
}

// ============================================================================
// Dot matrix overlay effect
// ============================================================================

pub(super) fn emit_dot_matrix(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.dot_matrix.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (cr, cg, cb) = ctx.effects.dot_matrix.color;
    let spacing = ctx.effects.dot_matrix.spacing.max(4.0);
    let pulse = ctx.effects.dot_matrix.pulse_speed;
    let opacity = ctx.effects.dot_matrix.opacity;
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    let cols = (width / spacing) as u32 + 1;
    let rows = (height / spacing) as u32 + 1;
    for row in 0..rows {
        for col in 0..cols {
            let dx = col as f32 * spacing;
            let dy = row as f32 * spacing;
            let phase = (row as f32 * 0.3 + col as f32 * 0.2 + now * pulse).sin() * 0.5 + 0.5;
            let dot_size = 2.0 * phase + 0.5;
            let alpha = opacity * phase;
            let c = Color::new(cr, cg, cb, alpha);
            push_rect(
                &mut verts,
                dx - dot_size / 2.0,
                dy - dot_size / 2.0,
                dot_size,
                dot_size,
                &c,
            );
        }
    }
    verts
}

// ============================================================================
// Concentric rings overlay effect
// ============================================================================

pub(super) fn emit_concentric_rings(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.concentric_rings.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (cr, cg, cb) = ctx.effects.concentric_rings.color;
    let spacing = ctx.effects.concentric_rings.spacing.max(10.0);
    let speed = ctx.effects.concentric_rings.expansion_speed;
    let opacity = ctx.effects.concentric_rings.opacity;
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let cx = width / 2.0;
    let cy = height / 2.0;
    let max_radius = (cx * cx + cy * cy).sqrt();
    let mut verts: Vec<RectVertex> = Vec::new();
    let ring_count = (max_radius / spacing) as u32 + 2;
    let phase = now * speed * spacing;
    for i in 0..ring_count {
        let r = (i as f32 * spacing + phase) % (max_radius + spacing);
        if r < 1.0 {
            continue;
        }
        let thickness = 1.5;
        let alpha = opacity * (1.0 - r / max_radius);
        if alpha < 0.001 {
            continue;
        }
        let c = Color::new(cr, cg, cb, alpha);
        // Approximate circle with axis-aligned rect segments
        let segments = (r * 0.5).max(16.0) as u32;
        for s in 0..segments {
            let angle = (s as f32 / segments as f32) * std::f32::consts::TAU;
            let px = cx + angle.cos() * r;
            let py = cy + angle.sin() * r;
            push_rect(
                &mut verts,
                px - thickness / 2.0,
                py - thickness / 2.0,
                thickness,
                thickness,
                &c,
            );
        }
    }
    verts
}

// ============================================================================
// Zigzag pattern overlay effect
// ============================================================================

pub(super) fn emit_zigzag(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.zigzag_pattern.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (cr, cg, cb) = ctx.effects.zigzag_pattern.color;
    let amplitude = ctx.effects.zigzag_pattern.amplitude;
    let freq = ctx.effects.zigzag_pattern.frequency;
    let speed = ctx.effects.zigzag_pattern.speed;
    let opacity = ctx.effects.zigzag_pattern.opacity;
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    let line_spacing = 30.0f32;
    let line_count = (height / line_spacing) as u32 + 1;
    for line in 0..line_count {
        let base_y = line as f32 * line_spacing;
        let segments = (width / 4.0) as u32;
        for s in 0..segments {
            let x = s as f32 * 4.0;
            let phase = now * speed + line as f32 * 0.5;
            let zigzag = ((x * freq + phase).rem_euclid(2.0) - 1.0).abs() * 2.0 - 1.0;
            let y = base_y + zigzag * amplitude;
            let c = Color::new(cr, cg, cb, opacity);
            push_rect(&mut verts, x, y, 4.0, 1.0, &c);
        }
    }
    verts
}

// ============================================================================
// Tessellation overlay effect
// ============================================================================

pub(super) fn emit_tessellation(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.tessellation.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let (tr, tg, tb) = ctx.effects.tessellation.color;
    let size = ctx.effects.tessellation.tile_size;
    let rot = ctx.effects.tessellation.rotation;
    let alpha = ctx.effects.tessellation.opacity;
    let mut verts = Vec::new();

    let cols = (width / size) as i32 + 2;
    let rows = (height / (size * 0.866)) as i32 + 2;
    let line_w = 1.0;

    for row in 0..rows {
        let y = row as f32 * size * 0.866;
        let x_off = if row % 2 == 1 { size * 0.5 } else { 0.0 };

        for col in 0..cols {
            let cx = col as f32 * size + x_off;
            let cy = y;

            // Draw triangle edges (alternating up/down)
            let up = (row + col) % 2 == 0;
            let h = size * 0.866;

            if up {
                // Upward triangle: base at bottom
                for s in 0..10 {
                    let t = s as f32 / 10.0;
                    // Left edge
                    let px = cx + t * size * 0.5;
                    let py = cy + h - t * h;
                    let c = Color::new(tr, tg, tb, alpha);
                    push_rect(
                        &mut verts,
                        px + rot.sin(),
                        py + rot.cos(),
                        line_w,
                        line_w,
                        &c,
                    );
                    // Right edge
                    let px2 = cx + size - t * size * 0.5;
                    push_rect(
                        &mut verts,
                        px2 + rot.sin(),
                        py + rot.cos(),
                        line_w,
                        line_w,
                        &c,
                    );
                }
                // Base
                let c = Color::new(tr, tg, tb, alpha);
                push_rect(&mut verts, cx, cy + h, size, line_w, &c);
            } else {
                // Downward triangle
                for s in 0..10 {
                    let t = s as f32 / 10.0;
                    let px = cx + t * size * 0.5;
                    let py = cy + t * h;
                    let c = Color::new(tr, tg, tb, alpha);
                    push_rect(
                        &mut verts,
                        px + rot.sin(),
                        py + rot.cos(),
                        line_w,
                        line_w,
                        &c,
                    );
                    let px2 = cx + size - t * size * 0.5;
                    push_rect(
                        &mut verts,
                        px2 + rot.sin(),
                        py + rot.cos(),
                        line_w,
                        line_w,
                        &c,
                    );
                }
                let c = Color::new(tr, tg, tb, alpha);
                push_rect(&mut verts, cx, cy, size, line_w, &c);
            }
        }
    }

    verts
}

// ============================================================================
// Guilloche overlay effect
// ============================================================================

pub(super) fn emit_guilloche(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.guilloche.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (gr, gg, gb) = ctx.effects.guilloche.color;
    let curves = ctx.effects.guilloche.curve_count;
    let freq = ctx.effects.guilloche.wave_freq;
    let alpha = ctx.effects.guilloche.opacity;
    let mut verts = Vec::new();

    let cx = width / 2.0;
    let cy = height / 2.0;

    for curve in 0..curves {
        let phase = curve as f32 * std::f32::consts::TAU / curves as f32;
        let segments = 200;

        for s in 0..segments {
            let t = s as f32 / segments as f32 * std::f32::consts::TAU;
            let r1 = 50.0 + curve as f32 * 20.0;
            let r2 = 30.0;
            let k = 3.0 + curve as f32 * 0.5;

            let px =
                cx + (r1 - r2) * (t + phase).cos() + r2 * ((r1 / r2 - 1.0) * t + now * freq).cos();
            let py =
                cy + (r1 - r2) * (t + phase).sin() - r2 * ((r1 / r2 - 1.0) * t + now * freq).sin();

            let brightness = ((t * k + now * freq).sin() * 0.3 + 0.7).max(0.0);
            let c = Color::new(gr, gg, gb, alpha * brightness);
            push_rect(&mut verts, px - 0.5, py - 0.5, 1.0, 1.0, &c);
        }
    }

    verts
}

// ============================================================================
// Celtic knot overlay effect
// ============================================================================

pub(super) fn emit_celtic_knot(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.celtic_knot.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (kr, kg, kb) = ctx.effects.celtic_knot.color;
    let scale = ctx.effects.celtic_knot.scale;
    let speed = ctx.effects.celtic_knot.weave_speed;
    let alpha = ctx.effects.celtic_knot.opacity;
    let mut verts = Vec::new();

    // Draw interlocking loops in a grid
    let cols = (width / scale) as i32 + 2;
    let rows = (height / scale) as i32 + 2;
    let segments = 16;
    let line_w = 2.0;

    for row in 0..rows {
        for col in 0..cols {
            let cx = col as f32 * scale + scale / 2.0;
            let cy = row as f32 * scale + scale / 2.0;
            let r = scale * 0.35;

            // Draw a circle (loop) at each grid point
            for s in 0..segments {
                let angle = (s as f32 / segments as f32) * std::f32::consts::TAU + now * speed;
                let px = cx + angle.cos() * r;
                let py = cy + angle.sin() * r;
                let phase = ((angle * 2.0 + now * speed * 2.0).sin() * 0.3 + 0.7).max(0.0);
                let c = Color::new(kr, kg, kb, alpha * phase);
                push_rect(
                    &mut verts,
                    px - line_w / 2.0,
                    py - line_w / 2.0,
                    line_w,
                    line_w,
                    &c,
                );
            }

            // Interlocking connector to adjacent cell
            if col < cols - 1 {
                for s in 0..8 {
                    let t = s as f32 / 8.0;
                    let px = cx + r + t * (scale - 2.0 * r);
                    let py = cy + (t * std::f32::consts::PI).sin() * r * 0.3;
                    let c = Color::new(kr, kg, kb, alpha * 0.6);
                    push_rect(
                        &mut verts,
                        px - line_w / 2.0,
                        py - line_w / 2.0,
                        line_w,
                        line_w,
                        &c,
                    );
                }
            }
        }
    }

    verts
}

// ============================================================================
// Argyle pattern overlay effect
// ============================================================================

pub(super) fn emit_argyle(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.argyle_pattern.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let (ar, ag, ab) = ctx.effects.argyle_pattern.color;
    let ds = ctx.effects.argyle_pattern.diamond_size;
    let lw = ctx.effects.argyle_pattern.line_width;
    let alpha = ctx.effects.argyle_pattern.opacity;
    let mut verts = Vec::new();

    let cols = (width / ds) as i32 + 2;
    let rows = (height / ds) as i32 + 2;

    // Diamond fills (alternating)
    for row in 0..rows {
        for col in 0..cols {
            if (row + col) % 2 == 0 {
                let cx = col as f32 * ds + ds / 2.0;
                let cy = row as f32 * ds + ds / 2.0;
                // Approximate diamond with small filled rect at center
                let half = ds * 0.4;
                let c = Color::new(ar, ag, ab, alpha * 0.3);
                push_rect(&mut verts, cx - half / 2.0, cy - half / 2.0, half, half, &c);
            }
        }
    }

    // Diagonal lines (top-left to bottom-right)
    let diags = ((width + height) / ds) as i32 + 2;
    for d in 0..diags {
        let start_x = d as f32 * ds - height;
        let steps = (height / 2.0) as i32;
        for s in 0..steps {
            let t = s as f32 / steps as f32;
            let px = start_x + t * height;
            let py = t * height;
            let c = Color::new(ar, ag, ab, alpha * 0.5);
            push_rect(&mut verts, px, py, lw, lw, &c);
        }
    }

    // Diagonal lines (top-right to bottom-left)
    for d in 0..diags {
        let start_x = d as f32 * ds;
        let steps = (height / 2.0) as i32;
        for s in 0..steps {
            let t = s as f32 / steps as f32;
            let px = start_x - t * height;
            let py = t * height;
            let c = Color::new(ar, ag, ab, alpha * 0.5);
            push_rect(&mut verts, px, py, lw, lw, &c);
        }
    }

    verts
}

// ============================================================================
// Basket weave overlay effect
// ============================================================================

pub(super) fn emit_basket_weave(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.basket_weave.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let (wr, wg, wb) = ctx.effects.basket_weave.color;
    let sw = ctx.effects.basket_weave.strip_width;
    let spacing = ctx.effects.basket_weave.strip_spacing;
    let alpha = ctx.effects.basket_weave.opacity;
    let mut verts = Vec::new();

    // Horizontal strips
    let h_strips = (height / spacing) as i32 + 1;
    for i in 0..h_strips {
        let y = i as f32 * spacing;
        let block = (i % 2) as f32;
        let cols = (width / spacing) as i32 + 1;
        for j in 0..cols {
            let x = j as f32 * spacing + block * spacing / 2.0;
            let c = Color::new(wr, wg, wb, alpha);
            push_rect(&mut verts, x, y, spacing / 2.0, sw, &c);
        }
    }

    // Vertical strips
    let v_strips = (width / spacing) as i32 + 1;
    for j in 0..v_strips {
        let x = j as f32 * spacing;
        let block = (j % 2) as f32;
        let rows = (height / spacing) as i32 + 1;
        for i in 0..rows {
            let y = i as f32 * spacing + block * spacing / 2.0;
            let c = Color::new(wr, wg, wb, alpha * 0.7);
            push_rect(&mut verts, x, y, sw, spacing / 2.0, &c);
        }
    }

    verts
}

// ============================================================================
// Fish scale overlay effect
// ============================================================================

pub(super) fn emit_fish_scale(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.fish_scale.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let (fr, fg, fb) = ctx.effects.fish_scale.color;
    let size = ctx.effects.fish_scale.size;
    let offset = ctx.effects.fish_scale.row_offset;
    let alpha = ctx.effects.fish_scale.opacity;
    let mut verts = Vec::new();

    let rows = (height / (size * 0.75)) as i32 + 2;
    let cols = (width / size) as i32 + 2;

    for row in 0..rows {
        let y = row as f32 * size * 0.75;
        let x_off = if row % 2 == 1 { size * offset } else { 0.0 };

        for col in 0..cols {
            let cx = col as f32 * size + x_off;
            let cy = y;

            // Draw semicircle as series of small rects (arc approximation)
            let segments = 10;
            for s in 0..segments {
                let angle = std::f32::consts::PI * s as f32 / segments as f32;
                let ax = cx + angle.cos() * size / 2.0;
                let ay = cy - angle.sin() * size / 2.0;
                let c = Color::new(fr, fg, fb, alpha);
                push_rect(&mut verts, ax - 0.5, ay - 0.5, 1.0, 1.0, &c);
            }
        }
    }

    verts
}

// ============================================================================
// Trefoil knot overlay effect
// ============================================================================

pub(super) fn emit_trefoil_knot(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.trefoil_knot.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (kr, kg, kb) = ctx.effects.trefoil_knot.color;
    let knot_size = ctx.effects.trefoil_knot.size;
    let rot_speed = ctx.effects.trefoil_knot.rotation_speed;
    let alpha = ctx.effects.trefoil_knot.opacity;
    let mut verts = Vec::new();

    let cx = width / 2.0;
    let cy = height / 2.0;
    let segments = 120;
    let seg_width = 3.0;

    for i in 0..segments {
        let t = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let angle = now * rot_speed;
        // Trefoil knot parametric equations
        let x = (t.sin() + 2.0 * (2.0 * t).sin()) * knot_size * 0.5;
        let y = (t.cos() - 2.0 * (2.0 * t).cos()) * knot_size * 0.5;
        // Apply rotation
        let rx = x * angle.cos() - y * angle.sin() + cx;
        let ry = x * angle.sin() + y * angle.cos() + cy;

        let phase = (t * 3.0 + now * 2.0).sin() * 0.3 + 0.7;
        let c = Color::new(kr, kg, kb, alpha * phase);
        push_rect(
            &mut verts,
            rx - seg_width / 2.0,
            ry - seg_width / 2.0,
            seg_width,
            seg_width,
            &c,
        );
    }

    verts
}

// ============================================================================
// Herringbone pattern overlay effect
// ============================================================================

pub(super) fn emit_herringbone(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.herringbone_pattern.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let (hr, hg, hb) = ctx.effects.herringbone_pattern.color;
    let tw = ctx.effects.herringbone_pattern.tile_width;
    let th = ctx.effects.herringbone_pattern.tile_height;
    let alpha = ctx.effects.herringbone_pattern.opacity;
    let mut verts = Vec::new();

    let line_w = 1.0;
    let cols = (width / tw) as i32 + 2;
    let rows = (height / th) as i32 + 2;

    for row in 0..rows {
        for col in 0..cols {
            let x = col as f32 * tw;
            let y = row as f32 * th;
            let even_row = row % 2 == 0;

            if even_row {
                // Draw V-shape pointing right
                // Top diagonal: from (x, y) to (x + tw/2, y + th/2)
                let steps = (tw / 2.0) as i32;
                for s in 0..steps {
                    let t = s as f32 / steps as f32;
                    let px = x + t * tw / 2.0;
                    let py = y + t * th / 2.0;
                    let c = Color::new(hr, hg, hb, alpha);
                    push_rect(&mut verts, px, py, line_w, line_w, &c);
                }
                // Bottom diagonal: from (x, y + th) to (x + tw/2, y + th/2)
                for s in 0..steps {
                    let t = s as f32 / steps as f32;
                    let px = x + t * tw / 2.0;
                    let py = y + th - t * th / 2.0;
                    let c = Color::new(hr, hg, hb, alpha);
                    push_rect(&mut verts, px, py, line_w, line_w, &c);
                }
            } else {
                // Offset row: V-shape shifted
                let offset = tw / 2.0;
                let steps = (tw / 2.0) as i32;
                for s in 0..steps {
                    let t = s as f32 / steps as f32;
                    let px = x + offset + t * tw / 2.0;
                    let py = y + t * th / 2.0;
                    let c = Color::new(hr, hg, hb, alpha);
                    push_rect(&mut verts, px, py, line_w, line_w, &c);
                }
                for s in 0..steps {
                    let t = s as f32 / steps as f32;
                    let px = x + offset + t * tw / 2.0;
                    let py = y + th - t * th / 2.0;
                    let c = Color::new(hr, hg, hb, alpha);
                    push_rect(&mut verts, px, py, line_w, line_w, &c);
                }
            }
        }
    }

    verts
}

// ============================================================================
// Target reticle overlay effect
// ============================================================================

pub(super) fn emit_target_reticle(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.target_reticle.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (tr, tg, tb) = ctx.effects.target_reticle.color;
    let ring_count = ctx.effects.target_reticle.ring_count;
    let pulse = ctx.effects.target_reticle.pulse_speed;
    let opacity = ctx.effects.target_reticle.opacity;
    let cx = width / 2.0;
    let cy = height / 2.0;
    let mut verts = Vec::new();
    for ring in 0..ring_count {
        let base_r = 50.0 + ring as f32 * 60.0;
        let r = base_r + (now * pulse + ring as f32 * 0.5).sin() * 10.0;
        let segments = 60;
        for s in 0..segments {
            let angle = s as f32 * std::f32::consts::TAU / segments as f32;
            let x = cx + angle.cos() * r;
            let y = cy + angle.sin() * r;
            let alpha = opacity * (1.0 - ring as f32 / ring_count as f32 * 0.5);
            let c = Color::new(tr, tg, tb, alpha);
            push_rect(&mut verts, x - 1.0, y - 1.0, 2.0, 2.0, &c);
        }
    }
    // Crosshair lines
    let ch_c = Color::new(tr, tg, tb, opacity * 0.5);
    push_rect(&mut verts, cx - 1.0, 0.0, 2.0, height, &ch_c);
    push_rect(&mut verts, 0.0, cy - 1.0, width, 2.0, &ch_c);
    verts
}

// ============================================================================
// Plaid pattern overlay effect
// ============================================================================

pub(super) fn emit_plaid(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.plaid_pattern.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let (pr, pg, pb) = ctx.effects.plaid_pattern.color;
    let band_w = ctx.effects.plaid_pattern.band_width;
    let spacing = ctx.effects.plaid_pattern.band_spacing;
    let opacity = ctx.effects.plaid_pattern.opacity;
    let mut verts = Vec::new();
    // Horizontal bands
    let h_count = (height / spacing) as i32 + 1;
    for i in 0..h_count {
        let y = i as f32 * spacing;
        let c = Color::new(pr, pg, pb, opacity);
        push_rect(&mut verts, 0.0, y, width, band_w, &c);
    }
    // Vertical bands (lighter, for plaid cross effect)
    let v_count = (width / spacing) as i32 + 1;
    for i in 0..v_count {
        let x = i as f32 * spacing;
        let c = Color::new(pr * 0.8, pg * 0.8, pb, opacity * 0.7);
        push_rect(&mut verts, x, 0.0, band_w, height, &c);
    }
    verts
}

// ============================================================================
// Brick wall overlay effect
// ============================================================================

pub(super) fn emit_brick_wall(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.brick_wall.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let (br, bg, bb) = ctx.effects.brick_wall.color;
    let bw = ctx.effects.brick_wall.width;
    let bh = ctx.effects.brick_wall.height;
    let opacity = ctx.effects.brick_wall.opacity;
    let mut verts = Vec::new();
    let rows = (height / bh) as i32 + 1;
    let cols = (width / bw) as i32 + 2;
    let mortar = 2.0;
    for row in 0..rows {
        let offset = if row % 2 == 1 { bw / 2.0 } else { 0.0 };
        let y = row as f32 * bh;
        // Horizontal mortar line
        let mc = Color::new(br, bg, bb, opacity);
        push_rect(&mut verts, 0.0, y, width, mortar, &mc);
        for col in (-1)..cols {
            let x = col as f32 * bw + offset;
            // Vertical mortar line
            push_rect(&mut verts, x, y, mortar, bh, &mc);
        }
    }
    verts
}

// ============================================================================
// Sine wave overlay effect
// ============================================================================

pub(super) fn emit_sine_wave(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.sine_wave.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (sr, sg, sb) = ctx.effects.sine_wave.color;
    let amplitude = ctx.effects.sine_wave.amplitude;
    let wavelength = ctx.effects.sine_wave.wavelength;
    let speed = ctx.effects.sine_wave.speed;
    let opacity = ctx.effects.sine_wave.opacity;
    let mut verts = Vec::new();
    let wave_count = (height / 40.0) as i32 + 1;
    for w in 0..wave_count {
        let base_y = w as f32 * 40.0 + 20.0;
        let phase = now * speed + w as f32 * 0.5;
        let steps = (width / 3.0) as i32;
        for s in 0..steps {
            let x = s as f32 * 3.0;
            let y = base_y + (x / wavelength * std::f32::consts::TAU + phase).sin() * amplitude;
            if y >= 0.0 && y <= height {
                let c = Color::new(sr, sg, sb, opacity);
                push_rect(&mut verts, x, y, 2.0, 2.0, &c);
            }
        }
    }
    verts
}

// ============================================================================
// Rotating gear overlay effect
// ============================================================================

pub(super) fn emit_rotating_gear(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.rotating_gear.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (gr, gg, gb) = ctx.effects.rotating_gear.color;
    let gear_size = ctx.effects.rotating_gear.size;
    let speed = ctx.effects.rotating_gear.speed;
    let opacity = ctx.effects.rotating_gear.opacity;
    let cols = (width / (gear_size * 2.5)) as i32 + 1;
    let rows = (height / (gear_size * 2.5)) as i32 + 1;
    let mut verts = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let cx = col as f32 * gear_size * 2.5 + gear_size;
            let cy = row as f32 * gear_size * 2.5 + gear_size;
            let dir = if (row + col) % 2 == 0 { 1.0 } else { -1.0 };
            let angle_base = now * speed * dir;
            let teeth = 8;
            for t in 0..teeth {
                let a1 = angle_base + t as f32 * std::f32::consts::TAU / teeth as f32;
                let a2 = a1 + std::f32::consts::TAU / (teeth as f32 * 2.0);
                let inner_r = gear_size * 0.6;
                let outer_r = gear_size;
                let x1 = cx + a1.cos() * outer_r;
                let y1 = cy + a1.sin() * outer_r;
                let x2 = cx + a2.cos() * outer_r;
                let y2 = cy + a2.sin() * outer_r;
                let tooth_w = ((x2 - x1).abs()).max(2.0);
                let tooth_h = ((y2 - y1).abs()).max(2.0);
                let c = Color::new(gr, gg, gb, opacity);
                push_rect(&mut verts, x1.min(x2), y1.min(y2), tooth_w, tooth_h, &c);
                // Inner ring segment
                let ix1 = cx + a1.cos() * inner_r;
                let iy1 = cy + a1.sin() * inner_r;
                let ic = Color::new(gr, gg, gb, opacity * 0.7);
                push_rect(&mut verts, ix1 - 1.0, iy1 - 1.0, 2.0, 2.0, &ic);
            }
        }
    }
    verts
}

// ============================================================================
// Crosshatch pattern overlay effect
// ============================================================================

pub(super) fn emit_crosshatch(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.crosshatch_pattern.enabled {
        return Vec::new();
    }
    let width = ctx.renderer_width;
    let height = ctx.renderer_height;
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (cr, cg, cb) = ctx.effects.crosshatch_pattern.color;
    let spacing = ctx.effects.crosshatch_pattern.line_spacing;
    let angle_deg = ctx.effects.crosshatch_pattern.angle;
    let speed = ctx.effects.crosshatch_pattern.speed;
    let opacity = ctx.effects.crosshatch_pattern.opacity;
    let angle_rad = angle_deg * std::f32::consts::PI / 180.0;
    let offset = now * speed * 20.0;
    let mut verts = Vec::new();
    let diag = (width * width + height * height).sqrt();
    let line_count = (diag / spacing) as i32 + 2;
    // First set of diagonal lines
    for i in (-line_count)..line_count {
        let base = i as f32 * spacing + offset.rem_euclid(spacing);
        let dx = angle_rad.cos();
        let dy = angle_rad.sin();
        let perp_x = -dy;
        let perp_y = dx;
        let center_x = width / 2.0 + perp_x * (base - diag / 2.0);
        let center_y = height / 2.0 + perp_y * (base - diag / 2.0);
        let steps = 30;
        for s in 0..steps {
            let t = (s as f32 / steps as f32 - 0.5) * diag;
            let x = center_x + dx * t;
            let y = center_y + dy * t;
            if x >= -2.0 && x <= width + 2.0 && y >= -2.0 && y <= height + 2.0 {
                let c = Color::new(cr, cg, cb, opacity);
                push_rect(&mut verts, x, y, 1.0, 1.0, &c);
            }
        }
    }
    // Second set perpendicular
    let angle_rad2 = angle_rad + std::f32::consts::FRAC_PI_2;
    let dx2 = angle_rad2.cos();
    let dy2 = angle_rad2.sin();
    let perp_x2 = -dy2;
    let perp_y2 = dx2;
    for i in (-line_count)..line_count {
        let base = i as f32 * spacing - offset.rem_euclid(spacing);
        let center_x = width / 2.0 + perp_x2 * (base - diag / 2.0);
        let center_y = height / 2.0 + perp_y2 * (base - diag / 2.0);
        let steps = 30;
        for s in 0..steps {
            let t = (s as f32 / steps as f32 - 0.5) * diag;
            let x = center_x + dx2 * t;
            let y = center_y + dy2 * t;
            if x >= -2.0 && x <= width + 2.0 && y >= -2.0 && y <= height + 2.0 {
                let c = Color::new(cr, cg, cb, opacity);
                push_rect(&mut verts, x, y, 1.0, 1.0, &c);
            }
        }
    }
    verts
}

// ============================================================================
// Hex grid overlay effect
// ============================================================================

pub(super) fn emit_hex_grid(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.hex_grid.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (hr, hg, hb) = ctx.effects.hex_grid.color;
    let hop = ctx.effects.hex_grid.opacity;
    let cell = ctx.effects.hex_grid.cell_size.max(10.0);
    let pspd = ctx.effects.hex_grid.pulse_speed;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    let hex_h = cell;
    let hex_w = cell * 0.866;
    let cols = (fw / hex_w) as i32 + 2;
    let rows = (fh / (hex_h * 0.75)) as i32 + 2;
    let line_thick = 1.0;
    for row in 0..rows {
        for col in 0..cols {
            let cx = col as f32 * hex_w + if row % 2 == 1 { hex_w * 0.5 } else { 0.0 };
            let cy = row as f32 * hex_h * 0.75;
            let pulse = (0.6 + 0.4 * ((cx * 0.01 + cy * 0.01 + now * pspd).sin())).max(0.0);
            let alpha = hop * pulse;
            let c = Color::new(hr, hg, hb, alpha);
            // Draw 6 edges of hexagon as small rects
            for edge in 0..6 {
                let a1 =
                    edge as f32 / 6.0 * std::f32::consts::PI * 2.0 + std::f32::consts::PI / 6.0;
                let a2 = (edge + 1) as f32 / 6.0 * std::f32::consts::PI * 2.0
                    + std::f32::consts::PI / 6.0;
                let r = cell * 0.5;
                let x1 = cx + a1.cos() * r;
                let y1 = cy + a1.sin() * r;
                let x2 = cx + a2.cos() * r;
                let y2 = cy + a2.sin() * r;
                let mx = x1.min(x2);
                let my = y1.min(y2);
                let ew = (x2 - x1).abs().max(line_thick);
                let eh = (y2 - y1).abs().max(line_thick);
                push_rect(&mut verts, mx, my, ew, eh, &c);
            }
        }
    }
    verts
}

// ============================================================================
// Circuit board trace effect
// ============================================================================

pub(super) fn emit_circuit_board(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.circuit_trace.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (cr, cg, cb) = ctx.effects.circuit_trace.color;
    let cop = ctx.effects.circuit_trace.opacity;
    let tw = ctx.effects.circuit_trace.width;
    let spd = ctx.effects.circuit_trace.speed;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    // Generate pseudo-random circuit traces along edges
    let trace_count = 8;
    for i in 0..trace_count {
        let seed = i * 7919 + 13;
        let side = i % 4;
        let offset = (seed as f32 * 0.618).fract();
        let pulse = (0.5 + 0.5 * (now * spd * 2.0 + i as f32 * 1.5).sin()).max(0.0);
        let alpha = cop * pulse;
        let c = Color::new(cr, cg, cb, alpha);
        match side {
            0 => {
                // top
                let x = offset * fw;
                let len = 30.0 + (seed as f32 * 0.3).fract() * 60.0;
                push_rect(&mut verts, x, 0.0, len.min(fw - x), tw, &c);
                // Right-angle turn down
                let turn_len = 15.0 + (seed as f32 * 0.7).fract() * 25.0;
                push_rect(&mut verts, x + len.min(fw - x) - tw, 0.0, tw, turn_len, &c);
                // Junction dot
                let dot = tw * 2.0;
                let dc = Color::new(cr, cg, cb, alpha * 1.5);
                push_rect(&mut verts, x - dot / 2.0, -dot / 4.0, dot, dot, &dc);
            }
            1 => {
                // right
                let y = offset * fh;
                let len = 30.0 + (seed as f32 * 0.5).fract() * 60.0;
                push_rect(&mut verts, fw - tw, y, tw, len.min(fh - y), &c);
                let turn_len = 15.0 + (seed as f32 * 0.9).fract() * 25.0;
                push_rect(&mut verts, fw - turn_len, y, turn_len, tw, &c);
            }
            2 => {
                // bottom
                let x = offset * fw;
                let len = 30.0 + (seed as f32 * 0.4).fract() * 60.0;
                push_rect(&mut verts, x, fh - tw, len.min(fw - x), tw, &c);
                let turn_len = 15.0 + (seed as f32 * 0.6).fract() * 25.0;
                push_rect(&mut verts, x, fh - turn_len, tw, turn_len, &c);
            }
            _ => {
                // left
                let y = offset * fh;
                let len = 30.0 + (seed as f32 * 0.8).fract() * 60.0;
                push_rect(&mut verts, 0.0, y, tw, len.min(fh - y), &c);
                let turn_len = 15.0 + (seed as f32 * 0.2).fract() * 25.0;
                push_rect(&mut verts, 0.0, y + len.min(fh - y) - tw, turn_len, tw, &c);
            }
        }
    }
    verts
}

// ============================================================================
// Warp/distortion grid effect
// ============================================================================

pub(super) fn emit_warp_grid(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.warp_grid.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let (wr, wg, wb) = ctx.effects.warp_grid.color;
    let wop = ctx.effects.warp_grid.opacity;
    let density = ctx.effects.warp_grid.density.max(2) as f32;
    let amp = ctx.effects.warp_grid.amplitude;
    let spd = ctx.effects.warp_grid.speed;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let cell_w = fw / density;
    let cell_h = fh / density;
    let mut verts: Vec<RectVertex> = Vec::new();
    let line_thick = 1.0;
    // Vertical lines
    for i in 0..=(density as u32) {
        let base_x = i as f32 * cell_w;
        for seg in 0..20 {
            let sy = seg as f32 / 20.0 * fh;
            let dy = amp * (sy * 0.02 + now * spd * 2.0).sin();
            let dx = amp * (sy * 0.015 + now * spd * 1.5 + base_x * 0.01).cos();
            let c = Color::new(wr, wg, wb, wop * (0.5 + 0.5 * (sy * 0.03 + now).sin()));
            push_rect(&mut verts, base_x + dx, sy + dy, line_thick, fh / 20.0, &c);
        }
    }
    // Horizontal lines
    for j in 0..=(density as u32) {
        let base_y = j as f32 * cell_h;
        for seg in 0..20 {
            let sx = seg as f32 / 20.0 * fw;
            let dx = amp * (sx * 0.02 + now * spd * 1.8).sin();
            let dy = amp * (sx * 0.015 + now * spd * 1.3 + base_y * 0.01).cos();
            let c = Color::new(
                wr,
                wg,
                wb,
                wop * (0.5 + 0.5 * (sx * 0.03 + now * 0.7).cos()),
            );
            push_rect(&mut verts, sx + dx, base_y + dy, fw / 20.0, line_thick, &c);
        }
    }
    verts
}

// ============================================================================
// Prism/rainbow edge effect
// ============================================================================

pub(super) fn emit_prism_rainbow_edge(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.prism_edge.enabled {
        return Vec::new();
    }
    let now = std::time::Instant::now()
        .duration_since(ctx.aurora_start)
        .as_secs_f32();
    let pw = ctx.effects.prism_edge.width;
    let pop = ctx.effects.prism_edge.opacity;
    let sat = ctx.effects.prism_edge.saturation;
    let spd = ctx.effects.prism_edge.speed;
    let fw = ctx.renderer_width;
    let fh = ctx.renderer_height;
    let mut verts: Vec<RectVertex> = Vec::new();
    let num_bands = 30;
    // Helper: HSV to RGB (simplified)
    let hsv_to_rgb = |h: f32, s: f32, v: f32| -> (f32, f32, f32) {
        let h = h % 1.0;
        let i = (h * 6.0).floor() as i32;
        let f = h * 6.0 - i as f32;
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        match i % 6 {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        }
    };
    // Top edge
    for i in 0..num_bands {
        let t = i as f32 / num_bands as f32;
        let hue = (t + now * spd * 0.2) % 1.0;
        let (pr, pg, pb) = hsv_to_rgb(hue, sat, 1.0);
        let x = t * fw;
        let band_w = fw / num_bands as f32 + 1.0;
        let c = Color::new(pr, pg, pb, pop);
        push_rect(&mut verts, x, 0.0, band_w, pw, &c);
    }
    // Bottom edge
    for i in 0..num_bands {
        let t = i as f32 / num_bands as f32;
        let hue = (t + now * spd * 0.2 + 0.5) % 1.0;
        let (pr, pg, pb) = hsv_to_rgb(hue, sat, 1.0);
        let x = t * fw;
        let band_w = fw / num_bands as f32 + 1.0;
        let c = Color::new(pr, pg, pb, pop);
        push_rect(&mut verts, x, fh - pw, band_w, pw, &c);
    }
    // Left edge
    for i in 0..num_bands {
        let t = i as f32 / num_bands as f32;
        let hue = (t + now * spd * 0.15 + 0.25) % 1.0;
        let (pr, pg, pb) = hsv_to_rgb(hue, sat, 1.0);
        let y = t * fh;
        let band_h = fh / num_bands as f32 + 1.0;
        let c = Color::new(pr, pg, pb, pop);
        push_rect(&mut verts, 0.0, y, pw, band_h, &c);
    }
    // Right edge
    for i in 0..num_bands {
        let t = i as f32 / num_bands as f32;
        let hue = (t + now * spd * 0.15 + 0.75) % 1.0;
        let (pr, pg, pb) = hsv_to_rgb(hue, sat, 1.0);
        let y = t * fh;
        let band_h = fh / num_bands as f32 + 1.0;
        let c = Color::new(pr, pg, pb, pop);
        push_rect(&mut verts, fw - pw, y, pw, band_h, &c);
    }
    verts
}

#[cfg(test)]
#[path = "pattern_effects_test.rs"]
mod tests;
