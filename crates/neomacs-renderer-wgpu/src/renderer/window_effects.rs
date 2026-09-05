//! Window visual effect methods.
//!
//! Contains all window-related visual effects extracted from the main
//! render_frame_glyphs function.
//!
//! KNOWN BUG (latent, low priority) — top chrome ignored in content geometry.
//! ---------------------------------------------------------------------------
//! Throughout this file (and `ui_overlays.rs`) effects compute a window's
//! "content area" as `content_h = bounds.height - mode_line_height`, drawn
//! starting at `bounds.y` (the window's TOP edge). That subtracts the BOTTOM
//! chrome (mode-line) but NOT the TOP chrome (tab-line + header-line). So when
//! a window has a tab-line and/or header-line, the rect these effects fill
//! starts too high and bleeds up over that chrome band.
//!
//! Symptom: with an opt-in effect enabled (file-type tint, non-selected dim,
//! accent strip, modified-indicator strip, focus ring, shadows, ...) AND a
//! tab-line/header-line present, the effect tints/dims/covers the
//! tab-line/header-line instead of staying inside the buffer text area. Purely
//! cosmetic; text still renders. It is LATENT because every such effect is
//! off by default, so nothing triggers it in a `-Q` session.
//!
//! Why it exists: the renderer threads a single scalar `mode_line_height`
//! through ~46 call sites and the top-chrome heights (`tab_line_height`,
//! `header_line_height`, already present on `WindowInfo`) are used 0 times.
//! There is no single place that knows "where the buffer text actually lives",
//! so each effect re-derives it ad hoc and only the bottom case was handled.
//!
//! IDEAL ABSTRACTION (when this is fixed): make chrome geometry typed RECTS,
//! computed once on `WindowInfo`, instead of a scalar subtracted everywhere:
//!
//! ```ignore
//! impl WindowInfo {
//!     fn top_chrome_height(&self) -> f32 {            // tab-line + header-line
//!         self.tab_line_height.max(0.0) + self.header_line_height.max(0.0)
//!     }
//!     fn content_rect(&self) -> Rect {                // where buffer text lives
//!         let top = self.top_chrome_height();
//!         let bottom = self.mode_line_height.max(0.0);
//!         Rect { x: self.bounds.x, y: self.bounds.y + top,
//!                width: self.bounds.width,
//!                height: (self.bounds.height - top - bottom).max(0.0) }
//!     }
//!     fn tab_line_rect(&self)    -> Option<Rect> { /* top band            */ }
//!     fn header_line_rect(&self) -> Option<Rect> { /* below the tab-line  */ }
//!     fn mode_line_rect(&self)   -> Option<Rect> { /* bottom band         */ }
//! }
//! ```
//!
//! Then every `bounds.height - mode_line_height` (from `bounds.y`) site below
//! collapses to `info.content_rect()`, and chrome backgrounds/separators use
//! the named band rects. Text drawing stays role-agnostic (it already is);
//! only this background/clip/effect layer needs the typed rects. That fixes
//! the header-line case for free and removes the scalar-threaded-everywhere
//! foot-gun. The fix is mechanical but must be verified visually with each
//! effect toggled ON and a tab-line/header-line present, so it is deferred to
//! a deliberate renderer chrome-geometry cleanup pass rather than done blind.
//!
//! Offending sites are tagged `FIXME(chrome-insets)` below.

use super::super::vertex::RectVertex;
use super::effect_common::{EffectCtx, ambient_cycle_phase, effect_entity_seed, push_rect};
use super::{
    ClickHaloEntry, CursorGhostEntry, EdgeGlowEntry, EdgeSnapEntry, HeatMapEntry, RainDrop,
    ScrollMomentumEntry, ScrollVelocityFadeEntry, WindowFadeEntry,
};
use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::frame_glyphs::{FrameGlyph, MaterializedFaceData};
use neomacs_display_protocol::frame_time::EventTime;
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::{Color, Rect};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Private helper: extension_to_color (copied from overlays.rs)
// ---------------------------------------------------------------------------

fn extension_to_color(ext: &str) -> (f32, f32, f32) {
    match ext {
        "rs" => (0.8, 0.3, 0.1),
        "el" | "lisp" | "scm" => (0.6, 0.2, 0.8),
        "c" | "h" => (0.2, 0.5, 0.8),
        "cpp" | "cc" | "hpp" => (0.2, 0.4, 0.7),
        "py" => (0.2, 0.6, 0.2),
        "js" | "jsx" => (0.9, 0.8, 0.2),
        "ts" | "tsx" => (0.2, 0.5, 0.9),
        "rb" => (0.8, 0.2, 0.2),
        "go" => (0.0, 0.6, 0.7),
        "java" => (0.7, 0.3, 0.1),
        "html" | "htm" => (0.9, 0.3, 0.2),
        "css" | "scss" => (0.2, 0.4, 0.9),
        "json" | "yaml" | "yml" | "toml" => (0.5, 0.5, 0.5),
        "md" | "org" | "txt" => (0.4, 0.7, 0.4),
        "sh" | "bash" | "zsh" => (0.3, 0.7, 0.3),
        _ => {
            let mut hash: u32 = 5381;
            for byte in ext.bytes() {
                hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
            }
            let hue = (hash % 360) as f32 / 360.0;
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

// ===========================================================================
// Pre-text window effects
// ===========================================================================

/// Buffer modified border indicator: colored strip on left edge of modified windows.
pub(super) fn emit_modified_indicator(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.modified_indicator.enabled {
        return Vec::new();
    }
    let (mr, mg, mb) = ctx.effects.modified_indicator.color;
    let malpha = ctx.effects.modified_indicator.opacity;
    let mc = Color::new(mr, mg, mb, malpha);
    let mw = ctx.effects.modified_indicator.width;
    let mut verts: Vec<RectVertex> = Vec::new();
    for win_info in &ctx.frame_glyphs.window_infos {
        if win_info.modified && !win_info.is_minibuffer {
            let wb = &win_info.bounds;
            let mode_line_h = win_info.mode_line_height;
            // FIXME(chrome-insets): ignores top chrome (tab/header line); strip
            // starts at wb.y and bleeds over it. Use win_info.content_rect(). See module note.
            let content_h = wb.height - mode_line_h;
            if content_h > 0.0 {
                push_rect(&mut verts, wb.x, wb.y, mw, content_h, &mc);
            }
        }
    }
    verts
}

/// Typing heat map overlay: fading highlights at recent cursor positions.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_typing_heatmap(
    ctx: &EffectCtx,
    heatmap_entries: &mut Vec<HeatMapEntry>,
    prev_cursor: &mut Option<(f32, f32)>,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.typing_heatmap.enabled {
        return (Vec::new(), false);
    }
    let now = ctx.frame_sample.presentation_time();
    let fade_dur = std::time::Duration::from_millis(ctx.effects.typing_heatmap.fade_ms as u64);

    // Detect cursor movement and record heat entry
    if let Some(anim) = ctx.animated_cursor {
        let cur_pos = (anim.x, anim.y);
        if let Some(prev_pos) = *prev_cursor {
            let dx = (cur_pos.0 - prev_pos.0).abs();
            let dy = (cur_pos.1 - prev_pos.1).abs();
            if (dx > 0.5 || dy > 0.5) && dx < 200.0 && dy < 100.0 {
                heatmap_entries.push(HeatMapEntry {
                    x: prev_pos.0,
                    y: prev_pos.1,
                    width: anim.width,
                    height: anim.height,
                    started: now,
                });
            }
        }
        *prev_cursor = Some(cur_pos);
    }

    // Prune expired entries
    heatmap_entries.retain(|e| now.saturating_since(e.started) < fade_dur);

    let mut needs_redraw = false;
    let mut verts: Vec<RectVertex> = Vec::new();
    if !heatmap_entries.is_empty() {
        let (hr, hg, hb) = ctx.effects.typing_heatmap.color;
        let max_alpha = ctx.effects.typing_heatmap.opacity;
        for entry in heatmap_entries.iter() {
            let elapsed = now.saturating_since(entry.started);
            let t = (elapsed.as_secs_f32() / fade_dur.as_secs_f32()).min(1.0);
            let alpha = max_alpha * (1.0 - t);
            if alpha > 0.001 {
                let c = Color::new(hr, hg, hb, alpha);
                push_rect(&mut verts, entry.x, entry.y, entry.width, entry.height, &c);
            }
        }
        if !heatmap_entries.is_empty() {
            needs_redraw = true;
        }
    }
    (verts, needs_redraw)
}

/// Inactive window stained glass effect: colored tint per buffer hash.
pub(super) fn emit_stained_glass(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.stained_glass.enabled {
        return Vec::new();
    }
    let sg_opacity = ctx.effects.stained_glass.opacity;
    let sg_sat = ctx.effects.stained_glass.saturation;
    let mut verts: Vec<RectVertex> = Vec::new();
    for win_info in &ctx.frame_glyphs.window_infos {
        if !win_info.selected && !win_info.is_minibuffer {
            let wb = &win_info.bounds;
            let mode_h = win_info.mode_line_height;
            // FIXME(chrome-insets): ignores top chrome (tab/header line); region
            // starts at wb.y and bleeds over it. Use win_info.content_rect(). See module note.
            let content_h = wb.height - mode_h;
            if content_h > 0.0 {
                let hash = win_info.buffer_id.wrapping_mul(2654435761) >> 16;
                let hue = (hash % 360) as f32;
                let c = sg_sat;
                let h_prime = hue / 60.0;
                let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
                let (r1, g1, b1) = match h_prime as u32 {
                    0 => (c, x, 0.0),
                    1 => (x, c, 0.0),
                    2 => (0.0, c, x),
                    3 => (0.0, x, c),
                    4 => (x, 0.0, c),
                    _ => (c, 0.0, x),
                };
                let m = 0.5 - c / 2.0;
                let color = Color::new(r1 + m, g1 + m, b1 + m, sg_opacity);
                push_rect(&mut verts, wb.x, wb.y, wb.width, content_h, &color);
            }
        }
    }
    verts
}

/// Focus gradient border: vertical color gradient along selected window edges.
pub(super) fn emit_focus_gradient_border(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.focus_gradient_border.enabled {
        return Vec::new();
    }
    let bw = ctx.effects.focus_gradient_border.width;
    let (tr, tg, tb) = ctx.effects.focus_gradient_border.top_color;
    let (br2, bg2, bb2) = ctx.effects.focus_gradient_border.bot_color;
    let alpha = ctx.effects.focus_gradient_border.opacity;
    let mut verts: Vec<RectVertex> = Vec::new();
    for win_info in &ctx.frame_glyphs.window_infos {
        if !win_info.selected || win_info.is_minibuffer {
            continue;
        }
        let b = &win_info.bounds;
        let strips = b.height.ceil() as u32;
        for s in 0..strips {
            let t = s as f32 / strips as f32;
            let r = tr + (br2 - tr) * t;
            let g = tg + (bg2 - tg) * t;
            let b_color = tb + (bb2 - tb) * t;
            let c = Color::new(r, g, b_color, alpha);
            let y = b.y + s as f32;
            // Left edge
            push_rect(&mut verts, b.x, y, bw, 1.0, &c);
            // Right edge
            push_rect(&mut verts, b.x + b.width - bw, y, bw, 1.0, &c);
        }
        // Top
        let tc = Color::new(tr, tg, tb, alpha);
        push_rect(&mut verts, b.x, b.y, b.width, bw, &tc);
        // Bottom
        let bc = Color::new(br2, bg2, bb2, alpha);
        push_rect(&mut verts, b.x, b.y + b.height - bw, b.width, bw, &bc);
    }
    verts
}

/// Window depth shadow layers: multi-layer drop shadow at window edges.
pub(super) fn emit_window_depth_shadow(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.depth_shadow.enabled {
        return Vec::new();
    }
    let (dr, dg, db) = ctx.effects.depth_shadow.color;
    let mut verts: Vec<RectVertex> = Vec::new();
    for win_info in &ctx.frame_glyphs.window_infos {
        if win_info.is_minibuffer {
            continue;
        }
        let b = &win_info.bounds;
        for layer in (1..=ctx.effects.depth_shadow.layers).rev() {
            let off = layer as f32 * ctx.effects.depth_shadow.offset;
            let alpha = ctx.effects.depth_shadow.opacity
                * (1.0 - (layer - 1) as f32 / ctx.effects.depth_shadow.layers as f32);
            let c = Color::new(dr, dg, db, alpha);
            // Bottom edge shadow
            push_rect(&mut verts, b.x + off, b.y + b.height, b.width, off, &c);
            // Right edge shadow
            push_rect(&mut verts, b.x + b.width, b.y + off, off, b.height, &c);
        }
    }
    verts
}

/// Mode-line gradient background: horizontal color gradient across mode-line.
pub(super) fn emit_mode_line_gradient(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.mode_line_gradient.enabled {
        return Vec::new();
    }
    let (lr, lg, lb) = ctx.effects.mode_line_gradient.left_color;
    let (rr, rg, rb) = ctx.effects.mode_line_gradient.right_color;
    let alpha = ctx.effects.mode_line_gradient.opacity;
    let mut verts: Vec<RectVertex> = Vec::new();
    for win_info in &ctx.frame_glyphs.window_infos {
        if win_info.is_minibuffer {
            continue;
        }
        let b = &win_info.bounds;
        let ml_h = win_info.mode_line_height;
        if ml_h < 1.0 {
            continue;
        }
        let ml_y = b.y + b.height - ml_h;
        let strips = b.width.ceil() as u32;
        let strip_w = (b.width / strips.max(1) as f32).max(1.0);
        let actual_strips = (b.width / strip_w).ceil() as u32;
        for s in 0..actual_strips {
            let t = s as f32 / actual_strips as f32;
            let r = lr + (rr - lr) * t;
            let g = lg + (rg - lg) * t;
            let b_c = lb + (rb - lb) * t;
            let c = Color::new(r, g, b_c, alpha);
            push_rect(
                &mut verts,
                b.x + s as f32 * strip_w,
                ml_y,
                strip_w,
                ml_h,
                &c,
            );
        }
    }
    verts
}

/// Window corner fold effect: triangular fold in top-right corner.
pub(super) fn emit_window_corner_fold(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.corner_fold.enabled {
        return Vec::new();
    }
    let fold_size = ctx.effects.corner_fold.size;
    let (fr, fg, fb) = ctx.effects.corner_fold.color;
    let fold_alpha = ctx.effects.corner_fold.opacity;
    let mut verts: Vec<RectVertex> = Vec::new();
    for win_info in &ctx.frame_glyphs.window_infos {
        if win_info.is_minibuffer {
            continue;
        }
        let b = &win_info.bounds;
        let steps = fold_size.ceil() as u32;
        for i in 0..steps {
            let t = i as f32 / steps as f32;
            let strip_w = fold_size * (1.0 - t);
            let strip_x = b.x + b.width - strip_w;
            let strip_y = b.y + i as f32;
            let alpha = fold_alpha * (1.0 - t * 0.3);
            let c = Color::new(fr, fg, fb, alpha);
            push_rect(&mut verts, strip_x, strip_y, strip_w, 1.0, &c);
        }
    }
    verts
}

/// Frosted window border effect: layered translucent border.
pub(super) fn emit_frosted_window_border(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.frosted_border.enabled {
        return Vec::new();
    }
    let bw = ctx.effects.frosted_border.width;
    let (fbr, fbg, fbb) = ctx.effects.frosted_border.color;
    let layers = 4u32;
    let mut verts: Vec<RectVertex> = Vec::new();
    for win_info in &ctx.frame_glyphs.window_infos {
        if win_info.is_minibuffer {
            continue;
        }
        let b = &win_info.bounds;
        for layer in 0..layers {
            let t = layer as f32 / layers as f32;
            let layer_w = bw * (1.0 - t * 0.5);
            let alpha = ctx.effects.frosted_border.opacity * (1.0 - t * 0.7);
            let c = Color::new(fbr, fbg, fbb, alpha);
            let inset = layer as f32 * (bw / layers as f32);
            // Top
            push_rect(
                &mut verts,
                b.x + inset,
                b.y + inset,
                b.width - inset * 2.0,
                layer_w / layers as f32,
                &c,
            );
            // Bottom
            push_rect(
                &mut verts,
                b.x + inset,
                b.y + b.height - inset - layer_w / layers as f32,
                b.width - inset * 2.0,
                layer_w / layers as f32,
                &c,
            );
            // Left
            push_rect(
                &mut verts,
                b.x + inset,
                b.y + inset,
                layer_w / layers as f32,
                b.height - inset * 2.0,
                &c,
            );
            // Right
            push_rect(
                &mut verts,
                b.x + b.width - inset - layer_w / layers as f32,
                b.y + inset,
                layer_w / layers as f32,
                b.height - inset * 2.0,
                &c,
            );
        }
    }
    verts
}

/// Window breathing border animation: pulsing border opacity.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_window_breathing_border(ctx: &EffectCtx) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.breathing_border.enabled {
        return (Vec::new(), false);
    }
    // Free-running: the breath has no trigger event, so its phase is the
    // session-start anchor aged against this frame's sample, one full breath
    // every `cycle_ms` (3s by default).
    let phase = ambient_cycle_phase(ctx, ctx.effects.breathing_border.cycle_ms);
    let breath = ((phase * std::f64::consts::TAU).sin() * 0.5 + 0.5) as f32;
    let alpha = ctx.effects.breathing_border.min_opacity
        + breath
            * (ctx.effects.breathing_border.max_opacity - ctx.effects.breathing_border.min_opacity);
    let (br, bg, bb) = ctx.effects.breathing_border.color;
    let c = Color::new(br, bg, bb, alpha);
    let border_w = 2.0_f32;
    let mut verts: Vec<RectVertex> = Vec::new();
    for win_info in &ctx.frame_glyphs.window_infos {
        if win_info.is_minibuffer {
            continue;
        }
        let b = &win_info.bounds;
        push_rect(&mut verts, b.x, b.y, b.width, border_w, &c);
        push_rect(
            &mut verts,
            b.x,
            b.y + b.height - border_w,
            b.width,
            border_w,
            &c,
        );
        push_rect(&mut verts, b.x, b.y, border_w, b.height, &c);
        push_rect(
            &mut verts,
            b.x + b.width - border_w,
            b.y,
            border_w,
            b.height,
            &c,
        );
    }
    (verts, true)
}

/// Window scanline (CRT) effect: horizontal lines across the full frame.
pub(super) fn emit_window_scanline(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.scanlines.enabled {
        return Vec::new();
    }
    let spacing = ctx.effects.scanlines.spacing.max(1) as f32;
    let (sr, sg, sb) = ctx.effects.scanlines.color;
    let sa = ctx.effects.scanlines.opacity;
    let c = Color::new(sr, sg, sb, sa);
    let mut verts: Vec<RectVertex> = Vec::new();
    let h = ctx.renderer_height;
    let w = ctx.renderer_width;
    let mut y = 0.0_f32;
    while y < h {
        push_rect(&mut verts, 0.0, y, w, 1.0, &c);
        y += spacing;
    }
    verts
}

/// Cursor ghost afterimage effect.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_cursor_ghost(
    ctx: &EffectCtx,
    ghost_entries: &mut Vec<CursorGhostEntry>,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.cursor_ghost.enabled {
        return (Vec::new(), false);
    }
    let now = ctx.frame_sample.presentation_time();
    let fade_dur = std::time::Duration::from_millis(ctx.effects.cursor_ghost.fade_ms as u64);

    // Detect cursor movement and spawn ghost
    if let Some(anim) = ctx.animated_cursor {
        let should_spawn = ghost_entries.is_empty()
            || ghost_entries.last().is_none_or(|last| {
                let dx = (anim.x - last.x).abs();
                let dy = (anim.y - last.y).abs();
                (dx > 2.0 || dy > 2.0) && now.saturating_since(last.started).as_millis() > 30
            });
        if should_spawn {
            ghost_entries.push(CursorGhostEntry {
                x: anim.x,
                y: anim.y,
                width: anim.width,
                height: anim.height,
                started: now,
            });
            while ghost_entries.len() > ctx.effects.cursor_ghost.count as usize * 2 {
                ghost_entries.remove(0);
            }
        }
    }

    // Prune expired
    ghost_entries.retain(|e| now.saturating_since(e.started) < fade_dur);

    let mut verts: Vec<RectVertex> = Vec::new();
    let mut needs_redraw = false;
    if !ghost_entries.is_empty() {
        let (gr, gg, gb) = ctx.effects.cursor_ghost.color;
        let drift = ctx.effects.cursor_ghost.drift;
        for entry in ghost_entries.iter() {
            let elapsed = now.saturating_since(entry.started).as_secs_f32();
            let t = (elapsed / fade_dur.as_secs_f32()).min(1.0);
            let alpha = ctx.effects.cursor_ghost.opacity * (1.0 - t) * (1.0 - t);
            if alpha < 0.001 {
                continue;
            }
            let dy = -drift * t;
            let scale = 1.0 + t * 0.3;
            let gw = entry.width * scale;
            let gh = entry.height * scale;
            let gx = entry.x - (gw - entry.width) / 2.0;
            let gy = entry.y + dy - (gh - entry.height) / 2.0;
            let c = Color::new(gr, gg, gb, alpha);
            push_rect(&mut verts, gx, gy, gw, gh, &c);
        }
        needs_redraw = true;
    }
    (verts, needs_redraw)
}

/// Edge glow on scroll boundaries.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_edge_glow(
    ctx: &EffectCtx,
    edge_glow_entries: &mut Vec<EdgeGlowEntry>,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.edge_glow.enabled {
        return (Vec::new(), false);
    }
    let now = ctx.frame_sample.presentation_time();
    edge_glow_entries.retain(|e| now.saturating_since(e.started) < e.duration);

    let mut verts: Vec<RectVertex> = Vec::new();
    let mut needs_redraw = false;
    if !edge_glow_entries.is_empty() {
        let (gr, gg, gb) = ctx.effects.edge_glow.color;
        let gh = ctx.effects.edge_glow.height;
        for entry in edge_glow_entries.iter() {
            let t =
                now.saturating_since(entry.started).as_secs_f32() / entry.duration.as_secs_f32();
            let fade = (1.0 - t) * (1.0 - t);
            let base_alpha = ctx.effects.edge_glow.opacity * fade;
            let steps = 20u32;
            for i in 0..steps {
                let frac = i as f32 / steps as f32;
                let strip_h = gh / steps as f32;
                let alpha = base_alpha * (1.0 - frac);
                if alpha < 0.001 {
                    continue;
                }
                let y = if entry.at_top {
                    entry.bounds.y + frac * gh
                } else {
                    entry.bounds.y + entry.bounds.height - gh + frac * gh
                };
                let c = Color::new(gr, gg, gb, alpha);
                push_rect(
                    &mut verts,
                    entry.bounds.x,
                    y,
                    entry.bounds.width,
                    strip_h,
                    &c,
                );
            }
        }
        needs_redraw = true;
    }
    (verts, needs_redraw)
}

/// Rain/drip ambient effect.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_rain_effect(
    ctx: &EffectCtx,
    rain_drops: &mut Vec<RainDrop>,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.rain_effect.enabled {
        return (Vec::new(), false);
    }
    let fw = ctx.logical_w;
    let fh = ctx.logical_h;
    let dt = 1.0 / 60.0_f32;

    // Spawn drops if needed
    while rain_drops.len() < ctx.effects.rain_effect.drop_count as usize {
        // RNG SEED, NOT A TIME: `now.elapsed()` was read for its low bits, not
        // for a duration. The frame's time sample is constant across a frame,
        // so substituting it would drop every raindrop down one track. Seed
        // each drop from the frame counter and its own index instead.
        let h = effect_entity_seed(ctx.frame_seq, rain_drops.len() as u64);
        let x = ((h >> 16) & 0xFFFF) as f32 / 65535.0 * fw;
        let y = -(((h >> 32) & 0xFFFF) as f32) / 65535.0 * fh * 0.5;
        let speed_var = 0.7 + ((h >> 48) & 0xFFFF) as f32 / 65535.0 * 0.6;
        let length = 8.0 + ((h >> 8) & 0xFF) as f32 / 255.0 * 16.0;
        let op = ctx.effects.rain_effect.opacity * (0.5 + ((h >> 4) & 0xFF) as f32 / 255.0 * 0.5);
        rain_drops.push(RainDrop {
            x,
            y,
            speed: ctx.effects.rain_effect.speed * speed_var,
            length,
            opacity: op,
        });
    }

    // Update positions
    for drop in rain_drops.iter_mut() {
        drop.y += drop.speed * dt;
        if drop.y > fh {
            // Same RNG seed, for a drop recycling off the bottom: its x is its
            // identity, so it falls on a fresh track rather than repeating one.
            let h = effect_entity_seed(ctx.frame_seq, (drop.x * 1000.0) as u64);
            drop.x = ((h >> 16) & 0xFFFF) as f32 / 65535.0 * fw;
            drop.y = -drop.length;
            let speed_var = 0.7 + ((h >> 48) & 0xFFFF) as f32 / 65535.0 * 0.6;
            drop.speed = ctx.effects.rain_effect.speed * speed_var;
        }
    }

    // Render drops
    let (dr, dg, db) = ctx.effects.rain_effect.color;
    let mut verts: Vec<RectVertex> = Vec::new();
    for drop in rain_drops.iter() {
        let c = Color::new(dr, dg, db, drop.opacity);
        push_rect(&mut verts, drop.x, drop.y, 1.0, drop.length, &c);
    }
    (verts, true)
}

/// Aurora/northern lights effect.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_aurora_overlay(ctx: &EffectCtx) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.aurora.enabled {
        return (Vec::new(), false);
    }
    let now = ctx.frame_sample.presentation_time();
    let elapsed =
        now.saturating_since(ctx.aurora_start).as_secs_f64() * ctx.effects.aurora.speed as f64;
    let fw = ctx.logical_w;
    let ah = ctx.effects.aurora.height;
    let (r1, g1, b1) = ctx.effects.aurora.color1;
    let (r2, g2, b2) = ctx.effects.aurora.color2;
    let mut verts: Vec<RectVertex> = Vec::new();
    let strips = 60u32;
    let strip_w = fw / strips as f32;
    for i in 0..strips {
        let x = i as f32 * strip_w;
        let phase = elapsed + i as f64 * 0.15;
        let wave1 = (phase * 0.7).sin() as f32 * 0.5 + 0.5;
        let wave2 = (phase * 1.1 + 2.0).sin() as f32 * 0.5 + 0.5;
        let blend = (phase * 0.3 + i as f64 * 0.1).sin() as f32 * 0.5 + 0.5;
        let cr = r1 * (1.0 - blend) + r2 * blend;
        let cg = g1 * (1.0 - blend) + g2 * blend;
        let cb = b1 * (1.0 - blend) + b2 * blend;
        let height = ah * (0.3 + 0.7 * wave1);
        let alpha = ctx.effects.aurora.opacity * (0.3 + 0.7 * wave2);
        let sub_strips = 8u32;
        let sub_h = height / sub_strips as f32;
        for j in 0..sub_strips {
            let frac = j as f32 / sub_strips as f32;
            let y = frac * height;
            let strip_alpha = alpha * (1.0 - frac);
            if strip_alpha < 0.001 {
                continue;
            }
            let c = Color::new(cr, cg, cb, strip_alpha);
            push_rect(&mut verts, x, y, strip_w + 1.0, sub_h + 0.5, &c);
        }
    }
    (verts, true)
}

// ===========================================================================
// Post-text window effects
// ===========================================================================

/// Mode-line separators: solid line, shadow, or gradient above mode-line.
pub(super) fn emit_mode_line_separator(ctx: &EffectCtx) -> Vec<RectVertex> {
    if ctx.effects.mode_line_separator.style == 0 {
        return Vec::new();
    }
    let (cr, cg, cb) = ctx.effects.mode_line_separator.color;
    let sep_h = ctx.effects.mode_line_separator.height;
    let style = ctx.effects.mode_line_separator.style;
    let mut verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if info.mode_line_height > 0.0 && !info.is_minibuffer {
            let b = &info.bounds;
            let sep_y = b.y + b.height - info.mode_line_height - sep_h;

            match style {
                1 => {
                    let c = Color::new(cr, cg, cb, 0.6);
                    push_rect(&mut verts, b.x, sep_y, b.width, 1.0, &c);
                }
                2 => {
                    let layers = sep_h.ceil() as i32;
                    for i in 0..layers {
                        let t = i as f32 / layers as f32;
                        let alpha = 0.3 * (1.0 - t);
                        let c = Color::new(cr, cg, cb, alpha);
                        push_rect(&mut verts, b.x, sep_y + i as f32, b.width, 1.0, &c);
                    }
                }
                3 => {
                    let layers = sep_h.ceil() as i32;
                    for i in 0..layers {
                        let t = (layers - 1 - i) as f32 / layers as f32;
                        let alpha = 0.4 * t;
                        let c = Color::new(cr, cg, cb, alpha);
                        push_rect(&mut verts, b.x, sep_y + i as f32, b.width, 1.0, &c);
                    }
                }
                _ => {}
            }
        }
    }
    verts
}

/// Buffer-local accent color strip on left edge.
pub(super) fn emit_accent_strip(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.accent_strip.enabled {
        return Vec::new();
    }
    let strip_w = ctx.effects.accent_strip.width.max(1.0);
    let mut verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if info.is_minibuffer {
            continue;
        }
        let b = &info.bounds;
        let ext = info.buffer_file_name.rsplit('.').next().unwrap_or("");
        let (r, g, b_col) = extension_to_color(ext);
        // FIXME(chrome-insets): ignores top chrome (tab/header line); region
        // starts at b.y and bleeds over it. Use info.content_rect(). See module note.
        let strip_h = b.height - info.mode_line_height;
        if strip_h <= 0.0 {
            continue;
        }
        let c = Color::new(r, g, b_col, 0.8);
        push_rect(&mut verts, b.x, b.y, strip_w, strip_h, &c);
    }
    verts
}

/// Window background tint based on file type.
pub(super) fn emit_window_mode_tint(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.window_mode_tint.enabled {
        return Vec::new();
    }
    let tint_alpha = ctx.effects.window_mode_tint.opacity.clamp(0.0, 1.0);
    let mut verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if info.is_minibuffer {
            continue;
        }
        let b = &info.bounds;
        // FIXME(chrome-insets): ignores top chrome (tab/header line); region
        // starts at b.y and bleeds over it. Use info.content_rect(). See module note.
        let content_h = b.height - info.mode_line_height;
        if content_h <= 0.0 {
            continue;
        }
        let ext = info.buffer_file_name.rsplit('.').next().unwrap_or("");
        if ext.is_empty() || ext == info.buffer_file_name {
            continue;
        }
        let (r, g, b_col) = extension_to_color(ext);
        let c = Color::new(r, g, b_col, tint_alpha);
        push_rect(&mut verts, b.x, b.y, b.width, content_h, &c);
    }
    verts
}

/// Animated focus ring (marching ants) around selected window.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_focus_ring(
    ctx: &EffectCtx,
    focus_ring_start: EventTime,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.focus_ring.enabled {
        return (Vec::new(), false);
    }
    let elapsed = ctx
        .frame_sample
        .since_at_presentation(focus_ring_start)
        .as_secs_f32();
    let offset =
        (elapsed * ctx.effects.focus_ring.speed) % (ctx.effects.focus_ring.dash_length * 2.0);
    let dash = ctx.effects.focus_ring.dash_length;
    let period = dash * 2.0;
    let thickness = 2.0_f32;
    let (cr, cg, cb) = ctx.effects.focus_ring.color;
    let alpha = ctx.effects.focus_ring.opacity.clamp(0.0, 1.0);
    let c = Color::new(cr, cg, cb, alpha);

    let mut verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if !info.selected || info.is_minibuffer {
            continue;
        }
        let b = &info.bounds;
        // FIXME(chrome-insets): ignores top chrome (tab/header line); region
        // starts at b.y and bleeds over it. Use info.content_rect(). See module note.
        let content_h = b.height - info.mode_line_height;
        if content_h <= 0.0 {
            continue;
        }

        // Helper: generate dashes along a line segment
        let mut add_dashes = |x0: f32, y0: f32, x1: f32, y1: f32, horizontal: bool| {
            let length = if horizontal {
                (x1 - x0).abs()
            } else {
                (y1 - y0).abs()
            };
            let mut pos = -offset;
            while pos < length {
                let dash_start = pos.max(0.0);
                let dash_end = (pos + dash).min(length);
                if dash_end > dash_start {
                    if horizontal {
                        push_rect(
                            &mut verts,
                            x0.min(x1) + dash_start,
                            y0,
                            dash_end - dash_start,
                            thickness,
                            &c,
                        );
                    } else {
                        push_rect(
                            &mut verts,
                            x0,
                            y0.min(y1) + dash_start,
                            thickness,
                            dash_end - dash_start,
                            &c,
                        );
                    }
                }
                pos += period;
            }
        };

        // Top edge
        add_dashes(b.x, b.y, b.x + b.width, b.y, true);
        // Bottom edge (above mode-line)
        add_dashes(
            b.x,
            b.y + content_h - thickness,
            b.x + b.width,
            b.y + content_h - thickness,
            true,
        );
        // Left edge
        add_dashes(b.x, b.y, b.x, b.y + content_h, false);
        // Right edge
        add_dashes(
            b.x + b.width - thickness,
            b.y,
            b.x + b.width - thickness,
            b.y + content_h,
            false,
        );
    }

    let needs_redraw = !verts.is_empty();
    (verts, needs_redraw)
}

/// Window padding gradient: inner edge shading for depth.
pub(super) fn emit_window_padding_gradient(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.padding_gradient.enabled {
        return Vec::new();
    }
    let grad_w = ctx.effects.padding_gradient.width.max(1.0);
    let peak_alpha = ctx.effects.padding_gradient.opacity.clamp(0.0, 1.0);
    let (cr, cg, cb) = ctx.effects.padding_gradient.color;
    let steps = (grad_w as i32).max(2);
    let mut verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if info.is_minibuffer {
            continue;
        }
        let b = &info.bounds;
        // FIXME(chrome-insets): ignores top chrome (tab/header line); region
        // starts at b.y and bleeds over it. Use info.content_rect(). See module note.
        let content_h = b.height - info.mode_line_height;
        if content_h <= 0.0 {
            continue;
        }

        let step_size = grad_w / steps as f32;
        for i in 0..steps {
            let t = i as f32 / steps as f32;
            let alpha = peak_alpha * (1.0 - t) * (1.0 - t);
            if alpha < 0.005 {
                continue;
            }
            let offset = i as f32 * step_size;
            let c = Color::new(cr, cg, cb, alpha);

            // Top edge
            push_rect(&mut verts, b.x, b.y + offset, b.width, step_size, &c);
            // Bottom edge (above mode-line)
            let bot_y = b.y + content_h - offset - step_size;
            if bot_y > b.y {
                push_rect(&mut verts, b.x, bot_y, b.width, step_size, &c);
            }
            // Left edge
            push_rect(&mut verts, b.x + offset, b.y, step_size, content_h, &c);
            // Right edge
            let right_x = b.x + b.width - offset - step_size;
            if right_x > b.x {
                push_rect(&mut verts, right_x, b.y, step_size, content_h, &c);
            }
        }
    }
    verts
}

/// Smooth border color transition on focus change.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_border_transition(
    ctx: &EffectCtx,
    border_transitions: &mut Vec<(i64, bool, EventTime)>,
    prev_border_selected: &mut i64,
    border_transition_duration: std::time::Duration,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.border_transition.enabled || ctx.frame_glyphs.window_infos.len() <= 1 {
        return (Vec::new(), false);
    }
    let now = ctx.frame_sample.presentation_time();
    let (ar, ag, ab) = ctx.effects.border_transition.active_color;
    let duration = border_transition_duration;

    // Detect selection change
    let mut new_selected: Option<i64> = None;
    for info in &ctx.frame_glyphs.window_infos {
        if info.selected && !info.is_minibuffer {
            new_selected = Some(info.window_id.get());
            break;
        }
    }
    if let Some(sel_id) = new_selected {
        if *prev_border_selected != 0 && sel_id != *prev_border_selected {
            border_transitions.retain(|&(wid, _, _)| wid != *prev_border_selected && wid != sel_id);
            border_transitions.push((*prev_border_selected, false, now));
            border_transitions.push((sel_id, true, now));
        }
        *prev_border_selected = sel_id;
    }

    // Clean up expired transitions
    border_transitions.retain(|&(_, _, start)| now.saturating_since(start) < duration);

    let border_thickness = 2.0_f32;
    let mut verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if info.is_minibuffer {
            continue;
        }
        let b = &info.bounds;
        // FIXME(chrome-insets): ignores top chrome (tab/header line); region
        // starts at b.y and bleeds over it. Use info.content_rect(). See module note.
        let content_h = b.height - info.mode_line_height;
        if content_h <= 0.0 {
            continue;
        }

        let alpha = if let Some(&(_, becoming_active, start)) = border_transitions
            .iter()
            .find(|&&(wid, _, _)| wid == info.window_id.get())
        {
            let t = (now.saturating_since(start).as_secs_f32() / duration.as_secs_f32()).min(1.0);
            let eased = t * (2.0 - t);
            if becoming_active { eased } else { 1.0 - eased }
        } else if info.selected {
            1.0_f32
        } else {
            0.0_f32
        };

        if alpha < 0.01 {
            continue;
        }

        let c = Color::new(ar, ag, ab, alpha * 0.7);
        push_rect(&mut verts, b.x, b.y, b.width, border_thickness, &c);
        push_rect(
            &mut verts,
            b.x,
            b.y + content_h - border_thickness,
            b.width,
            border_thickness,
            &c,
        );
        push_rect(&mut verts, b.x, b.y, border_thickness, content_h, &c);
        push_rect(
            &mut verts,
            b.x + b.width - border_thickness,
            b.y,
            border_thickness,
            content_h,
            &c,
        );
    }

    let needs_redraw = !verts.is_empty();
    (verts, needs_redraw)
}

/// Frosted glass effect on mode-lines.
pub(super) fn emit_frosted_glass(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.frosted_glass.enabled {
        return Vec::new();
    }
    let frost_opacity = ctx.effects.frosted_glass.opacity.clamp(0.0, 1.0);
    let blur_r = ctx.effects.frosted_glass.blur.max(1.0);
    let mut verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if info.mode_line_height <= 0.0 || info.is_minibuffer {
            continue;
        }
        let b = &info.bounds;
        let ml_y = b.y + b.height - info.mode_line_height;
        let ml_h = info.mode_line_height;

        // Layer 1: Base frosted overlay
        let base_color = Color::new(1.0, 1.0, 1.0, frost_opacity * 0.15);
        push_rect(&mut verts, b.x, ml_y, b.width, ml_h, &base_color);

        // Layer 2-5: Offset blur layers
        let offsets = [
            (-blur_r, 0.0),
            (blur_r, 0.0),
            (0.0, -blur_r * 0.5),
            (0.0, blur_r * 0.5),
        ];
        let layer_alpha = frost_opacity * 0.06;
        for (dx, dy) in offsets {
            let c = Color::new(1.0, 1.0, 1.0, layer_alpha);
            let rx = (b.x + dx).max(b.x);
            let ry = (ml_y + dy).max(ml_y);
            let rw = b.width.min(b.x + b.width - rx);
            let rh = ml_h.min(ml_y + ml_h - ry);
            if rw > 0.0 && rh > 0.0 {
                push_rect(&mut verts, rx, ry, rw, rh, &c);
            }
        }

        // Layer 6: Grain/noise pattern
        let grain_size = 2.0_f32;
        let grain_alpha = frost_opacity * 0.04;
        let cols = (b.width / grain_size) as i32;
        let rows = (ml_h / grain_size) as i32;
        for row in 0..rows {
            for col in 0..cols {
                let hash =
                    ((row as u64).wrapping_mul(7919) + (col as u64).wrapping_mul(104729)) % 5;
                if hash == 0 {
                    let gx = b.x + col as f32 * grain_size;
                    let gy = ml_y + row as f32 * grain_size;
                    let c = Color::new(1.0, 1.0, 1.0, grain_alpha);
                    push_rect(&mut verts, gx, gy, grain_size, grain_size, &c);
                }
            }
        }

        // Top edge: bright line for glass edge highlight
        let edge_c = Color::new(1.0, 1.0, 1.0, frost_opacity * 0.3);
        push_rect(&mut verts, b.x, ml_y, b.width, 1.0, &edge_c);
    }
    verts
}

/// Noise/film grain texture overlay.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_noise_grain(
    ctx: &EffectCtx,
    noise_grain_frame: &mut u32,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.noise_grain.enabled {
        return (Vec::new(), false);
    }
    let grain_size = ctx.effects.noise_grain.size.max(1.0);
    let intensity = ctx.effects.noise_grain.intensity.clamp(0.0, 1.0);
    let frame_w = ctx.surface_width as f32 / ctx.scale_factor;
    let frame_h = ctx.surface_height as f32 / ctx.scale_factor;
    let cols = (frame_w / grain_size) as i32;
    let rows = (frame_h / grain_size) as i32;
    let frame_seed = *noise_grain_frame as u64;
    *noise_grain_frame = noise_grain_frame.wrapping_add(1);

    let mut verts: Vec<RectVertex> = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let hash = ((row as u64)
                .wrapping_mul(7919)
                .wrapping_add((col as u64).wrapping_mul(104729))
                .wrapping_add(frame_seed.wrapping_mul(31337)))
                % 97;
            if hash < 15 {
                let lum = if hash < 8 { 0.0 } else { 1.0 };
                let alpha = intensity * (hash as f32 / 15.0) * 0.5;
                let gx = col as f32 * grain_size;
                let gy = row as f32 * grain_size;
                let c = Color::new(lum, lum, lum, alpha);
                push_rect(&mut verts, gx, gy, grain_size, grain_size, &c);
            }
        }
    }
    (verts, true)
}

/// Idle screen dimming overlay.
pub(super) fn emit_idle_dimming(ctx: &EffectCtx, idle_dim_alpha: f32) -> Vec<RectVertex> {
    if idle_dim_alpha <= 0.001 {
        return Vec::new();
    }
    let frame_w = ctx.surface_width as f32 / ctx.scale_factor;
    let frame_h = ctx.surface_height as f32 / ctx.scale_factor;
    let dim_c = Color::new(0.0, 0.0, 0.0, idle_dim_alpha);
    let mut verts: Vec<RectVertex> = Vec::new();
    push_rect(&mut verts, 0.0, 0.0, frame_w, frame_h, &dim_c);
    verts
}

/// Focus mode: dim lines outside current paragraph.
pub(super) fn emit_focus_mode(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.focus_mode.enabled {
        return Vec::new();
    }

    // Find active cursor Y position
    let mut cursor_y: Option<f32> = None;
    let mut cursor_h: f32 = 0.0;
    if let Some(anim) = ctx.animated_cursor {
        cursor_y = Some(anim.y);
        cursor_h = anim.height;
    } else if let Some(cursor) = ctx.frame_glyphs.active_cursor() {
        cursor_y = Some(cursor.y);
        cursor_h = cursor.height;
    }

    let cy = match cursor_y {
        Some(cy) => cy,
        None => return Vec::new(),
    };

    // Find selected window bounds
    let mut sel_bounds: Option<&Rect> = None;
    for info in &ctx.frame_glyphs.window_infos {
        if info.selected && !info.is_minibuffer {
            sel_bounds = Some(&info.bounds);
            break;
        }
    }

    let bounds = match sel_bounds {
        Some(b) => b,
        None => return Vec::new(),
    };

    // Collect unique row Y positions within the selected window
    let mut row_ys: Vec<(f32, f32, bool)> = Vec::new();
    let mut last_y: f32 = -9999.0;
    let mut last_h: f32 = 0.0;
    let mut has_non_space = false;

    for glyph in &ctx.frame_glyphs.glyphs {
        if let FrameGlyph::Char {
            x,
            y,
            height,
            char: ch,
            row_role,
            ..
        } = glyph
        {
            if row_role.is_chrome() {
                continue;
            }
            if *x < bounds.x || *x >= bounds.x + bounds.width {
                continue;
            }
            if *y < bounds.y || *y >= bounds.y + bounds.height {
                continue;
            }
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
    let cursor_row_idx = row_ys
        .iter()
        .position(|(y, _, _)| (*y - cy).abs() < cursor_h);

    let cursor_idx = match cursor_row_idx {
        Some(idx) => idx,
        None => return Vec::new(),
    };

    // Search backward for paragraph start
    let mut para_start_y = bounds.y;
    for i in (0..cursor_idx).rev() {
        if !row_ys[i].2 {
            para_start_y = row_ys[i].0 + row_ys[i].1;
            break;
        }
    }

    // Search forward for paragraph end
    let mut para_end_y = bounds.y + bounds.height;
    for row in &row_ys[(cursor_idx + 1)..] {
        if !row.2 {
            para_end_y = row.0;
            break;
        }
    }

    // Draw dim overlays above and below the paragraph
    let dim_color = Color::new(0.0, 0.0, 0.0, ctx.effects.focus_mode.opacity);
    let mut verts: Vec<RectVertex> = Vec::new();

    if para_start_y > bounds.y + 1.0 {
        push_rect(
            &mut verts,
            bounds.x,
            bounds.y,
            bounds.width,
            para_start_y - bounds.y,
            &dim_color,
        );
    }
    if para_end_y < bounds.y + bounds.height - 1.0 {
        push_rect(
            &mut verts,
            bounds.x,
            para_end_y,
            bounds.width,
            bounds.y + bounds.height - para_end_y,
            &dim_color,
        );
    }
    verts
}

/// Inactive window dimming overlays with smooth fade.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_inactive_window_dimming(
    ctx: &EffectCtx,
    per_window_dim: &mut HashMap<i64, f32>,
    last_dim_tick: &mut EventTime,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.inactive_dim.enabled || ctx.frame_glyphs.window_infos.len() <= 1 {
        return (Vec::new(), false);
    }
    let now = ctx.frame_sample.presentation_time();
    let dt = now.saturating_since(*last_dim_tick).as_secs_f32().min(0.1);
    *last_dim_tick = now;
    let fade_speed = 8.0;

    let mut verts: Vec<RectVertex> = Vec::new();
    let mut any_transitioning = false;
    for info in &ctx.frame_glyphs.window_infos {
        let target = if info.selected {
            0.0
        } else {
            ctx.effects.inactive_dim.opacity
        };
        let current = per_window_dim
            .get(&info.window_id.get())
            .copied()
            .unwrap_or(target);
        let new_opacity = current + (target - current) * (1.0 - (-fade_speed * dt).exp());
        let new_opacity = if (new_opacity - target).abs() < 0.001 {
            target
        } else {
            new_opacity
        };
        per_window_dim.insert(info.window_id.get(), new_opacity);
        if (new_opacity - target).abs() > 0.0005 {
            any_transitioning = true;
        }
        if new_opacity > 0.001 {
            let dim_color = Color::new(0.0, 0.0, 0.0, new_opacity);
            let b = &info.bounds;
            push_rect(&mut verts, b.x, b.y, b.width, b.height, &dim_color);
        }
    }
    // Clean up windows that no longer exist
    let valid_ids: std::collections::HashSet<i64> = ctx
        .frame_glyphs
        .window_infos
        .iter()
        .map(|i| i.window_id.get())
        .collect();
    per_window_dim.retain(|k, _| valid_ids.contains(k));

    (verts, any_transitioning)
}

/// Inactive window color tint.
pub(super) fn emit_inactive_window_tint(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.inactive_tint.enabled || ctx.frame_glyphs.window_infos.len() <= 1 {
        return Vec::new();
    }
    let (tr, tg, tb) = ctx.effects.inactive_tint.color;
    let opacity = ctx.effects.inactive_tint.opacity.clamp(0.0, 1.0);
    let mut verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if info.selected {
            continue;
        }
        let b = &info.bounds;
        let tint_color = Color::new(tr, tg, tb, opacity);
        push_rect(&mut verts, b.x, b.y, b.width, b.height, &tint_color);
    }
    verts
}

/// Zen mode: draw margin overlays for centered content.
pub(super) fn emit_zen_mode(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.zen_mode.enabled {
        return Vec::new();
    }
    let content_pct = ctx.effects.zen_mode.content_width_pct.clamp(20.0, 100.0) / 100.0;
    let margin_alpha = ctx.effects.zen_mode.margin_opacity;
    let dim_color = Color::new(0.0, 0.0, 0.0, margin_alpha);
    let mut verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if info.is_minibuffer {
            continue;
        }
        let b = &info.bounds;
        // FIXME(chrome-insets): ignores top chrome (tab/header line); region
        // starts at b.y and bleeds over it. Use info.content_rect(). See module note.
        let content_h = b.height - info.mode_line_height;
        if content_h < 10.0 {
            continue;
        }

        let content_w = b.width * content_pct;
        let margin_w = (b.width - content_w) / 2.0;

        if margin_w > 2.0 {
            push_rect(&mut verts, b.x, b.y, margin_w, content_h, &dim_color);
            push_rect(
                &mut verts,
                b.x + b.width - margin_w,
                b.y,
                margin_w,
                content_h,
                &dim_color,
            );
        }

        if info.mode_line_height > 0.0 {
            let ml_dim = Color::new(0.0, 0.0, 0.0, margin_alpha * 0.5);
            push_rect(
                &mut verts,
                b.x,
                b.y + content_h,
                b.width,
                info.mode_line_height,
                &ml_dim,
            );
        }
    }
    verts
}

/// Search highlight pulse (glow on isearch face glyphs).
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_search_highlight(
    ctx: &EffectCtx,
    search_pulse_start: EventTime,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.search_pulse.enabled || ctx.effects.search_pulse.face_id == FaceId::new(0) {
        return (Vec::new(), false);
    }
    let target_face = ctx.effects.search_pulse.face_id;
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut found = false;
    let mut match_bg: Option<Color> = None;

    for glyph in &ctx.frame_glyphs.glyphs {
        if let FrameGlyph::Char {
            x,
            y,
            width,
            height,
            face_id,
            ..
        } = glyph
            && *face_id == target_face
        {
            min_x = min_x.min(*x);
            min_y = min_y.min(*y);
            max_x = max_x.max(*x + *width);
            max_y = max_y.max(*y + *height);
            found = true;
            if match_bg.is_none() {
                // Background was inlined as `Some(face.background)`.
                match_bg = Some(ctx.frame_glyphs.resolved_face(*face_id).bg);
            }
        }
    }

    if !found {
        return (Vec::new(), false);
    }

    let elapsed = ctx
        .frame_sample
        .since_at_presentation(search_pulse_start)
        .as_secs_f32();
    let phase = elapsed * 3.0 * std::f32::consts::PI;
    let pulse = (phase.sin() + 1.0) / 2.0;

    let (pr, pg, pb) = match match_bg {
        Some(c) => (c.r, c.g, c.b),
        None => (1.0, 0.8, 0.3),
    };

    let glow_alpha = 0.15 + 0.2 * pulse;
    let glow_pad = 4.0 + 3.0 * pulse;
    let glow_color = Color::new(pr, pg, pb, glow_alpha);

    let mut verts: Vec<RectVertex> = Vec::new();
    push_rect(
        &mut verts,
        min_x - glow_pad,
        min_y - glow_pad,
        (max_x - min_x) + glow_pad * 2.0,
        (max_y - min_y) + glow_pad * 2.0,
        &glow_color,
    );
    (verts, true)
}

/// Selection region glow highlight.
pub(super) fn emit_selection_glow(
    ctx: &EffectCtx,
    faces: &HashMap<FaceId, Face>,
) -> Vec<RectVertex> {
    if !ctx.effects.region_glow.enabled || ctx.effects.region_glow.face_id == FaceId::new(0) {
        return Vec::new();
    }
    let target_face = ctx.effects.region_glow.face_id;
    let glow_radius = ctx.effects.region_glow.radius.max(1.0);
    let glow_opacity = ctx.effects.region_glow.opacity.clamp(0.0, 1.0);

    // Collect per-row bounding boxes for region glyphs
    let mut row_bounds: Vec<(f32, f32, f32, f32)> = Vec::new();
    let mut current_row_y: f32 = -9999.0;
    let mut row_min_x: f32 = f32::MAX;
    let mut row_max_x: f32 = f32::MIN;
    let mut row_h: f32 = 0.0;

    for glyph in &ctx.frame_glyphs.glyphs {
        if let FrameGlyph::Char {
            x,
            y,
            width,
            height,
            face_id,
            ..
        } = glyph
            && *face_id == target_face
        {
            if (*y - current_row_y).abs() > 1.0 {
                if row_min_x < row_max_x {
                    row_bounds.push((row_min_x, current_row_y, row_max_x - row_min_x, row_h));
                }
                current_row_y = *y;
                row_min_x = *x;
                row_max_x = *x + *width;
                row_h = *height;
            } else {
                row_min_x = row_min_x.min(*x);
                row_max_x = row_max_x.max(*x + *width);
                row_h = row_h.max(*height);
            }
        }
    }
    if row_min_x < row_max_x {
        row_bounds.push((row_min_x, current_row_y, row_max_x - row_min_x, row_h));
    }

    if row_bounds.is_empty() {
        return Vec::new();
    }

    let region_color = faces
        .get(&target_face)
        .map(|f| (f.background.r, f.background.g, f.background.b))
        .unwrap_or((0.4, 0.6, 1.0));

    let mut verts: Vec<RectVertex> = Vec::new();
    let steps = (glow_radius as i32).max(2);

    for (rx, ry, rw, rh) in &row_bounds {
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let alpha = glow_opacity * (1.0 - t) * (1.0 - t);
            if alpha < 0.005 {
                continue;
            }
            let pad = t * glow_radius;
            let c = Color::new(region_color.0, region_color.1, region_color.2, alpha);
            push_rect(
                &mut verts,
                rx - pad,
                ry - pad,
                rw + pad * 2.0,
                rh + pad * 2.0,
                &c,
            );
        }
    }
    verts
}

/// Typing ripple effect.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_typing_ripple(
    ctx: &EffectCtx,
    active_ripples: &mut Vec<(f32, f32, EventTime)>,
    typing_ripple_duration: f32,
) -> (Vec<RectVertex>, bool) {
    if !ctx.effects.typing_ripple.enabled || active_ripples.is_empty() {
        return (Vec::new(), false);
    }
    let now = ctx.frame_sample.presentation_time();
    let duration = typing_ripple_duration;
    let max_r = ctx.effects.typing_ripple.max_radius;

    // Remove expired ripples
    active_ripples.retain(|&(_, _, t)| now.saturating_since(t).as_secs_f32() < duration);

    if active_ripples.is_empty() {
        return (Vec::new(), false);
    }

    let mut verts: Vec<RectVertex> = Vec::new();
    let segments = 32;

    for &(cx, cy, spawn_t) in active_ripples.iter() {
        let elapsed = now.saturating_since(spawn_t).as_secs_f32();
        let t = (elapsed / duration).min(1.0);

        let ease_t = 1.0 - (1.0 - t) * (1.0 - t);
        let radius = max_r * ease_t;
        let alpha = 0.4 * (1.0 - t);
        let ring_thickness = 1.5;

        let color = Color::new(0.5, 0.7, 1.0, alpha);

        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * 2.0 * std::f32::consts::PI;
            let px = cx + radius * angle.cos();
            let py = cy + radius * angle.sin();
            push_rect(
                &mut verts,
                px - ring_thickness / 2.0,
                py - ring_thickness / 2.0,
                ring_thickness,
                ring_thickness,
                &color,
            );
        }
    }
    let has_verts = !verts.is_empty();
    (verts, has_verts)
}

/// Minimap: code overview column on right side of each window.
pub(super) fn emit_minimap(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.minimap.enabled {
        return Vec::new();
    }
    let minimap_w = ctx.effects.minimap.width;
    let char_w = ctx.frame_glyphs.char_width.max(1.0);
    let char_h = ctx.frame_glyphs.char_height.max(1.0);
    let scale_x = 2.0_f32;
    let scale_y = 1.5_f32;

    let mut all_verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if info.is_minibuffer {
            continue;
        }

        let b = &info.bounds;
        // FIXME(chrome-insets): ignores top chrome (tab/header line); region
        // starts at b.y and bleeds over it. Use info.content_rect(). See module note.
        let content_h = b.height - info.mode_line_height;
        if content_h < 10.0 {
            continue;
        }

        let map_x = b.x + b.width - minimap_w;
        let map_y = b.y;
        let map_h = content_h;

        // Semi-transparent background
        let bg_color = Color::new(0.0, 0.0, 0.0, 0.15);
        push_rect(&mut all_verts, map_x, map_y, minimap_w, map_h, &bg_color);

        let mut mini_face_cache: Option<(FaceId, MaterializedFaceData)> = None;
        for glyph in &ctx.frame_glyphs.glyphs {
            if let FrameGlyph::Char {
                x,
                y,
                width,
                height: _,
                face_id,
                char: ch,
                row_role,
                ..
            } = glyph
            {
                if row_role.is_chrome() {
                    continue;
                }
                if *ch == ' ' || *ch == '\t' || *ch == '\n' {
                    continue;
                }
                let fg = match mini_face_cache {
                    Some((id, ref data)) if id == *face_id => data.fg,
                    _ => {
                        let data = ctx.frame_glyphs.resolved_face(*face_id);
                        mini_face_cache = Some((*face_id, data));
                        data.fg
                    }
                };
                if *x < b.x || *x >= b.x + b.width - minimap_w {
                    continue;
                }
                if *y < b.y || *y >= b.y + content_h {
                    continue;
                }

                let rel_x = (*x - b.x) / char_w;
                let rel_y = (*y - b.y) / char_h;
                let mini_x = map_x + 2.0 + rel_x * scale_x;
                let mini_y = map_y + rel_y * scale_y;

                if mini_x >= map_x + minimap_w - 1.0 {
                    continue;
                }
                if mini_y >= map_y + map_h - 1.0 {
                    continue;
                }

                let dot_w = ((*width / char_w) * scale_x).max(1.0).min(scale_x * 2.0);
                let dot_h = scale_y;
                let dot_color = Color::new(fg.r, fg.g, fg.b, 0.7);
                push_rect(&mut all_verts, mini_x, mini_y, dot_w, dot_h, &dot_color);
            }
        }

        // Viewport indicator
        if info.buffer_size > 0 {
            let start_frac = info.window_start as f32 / info.buffer_size as f32;
            let end_frac = (info.window_end as f32 / info.buffer_size as f32).min(1.0);
            let vp_y = map_y + start_frac * map_h;
            let vp_h = ((end_frac - start_frac) * map_h).max(4.0);
            let vp_color = Color::new(1.0, 1.0, 1.0, 0.1);
            push_rect(&mut all_verts, map_x, vp_y, minimap_w, vp_h, &vp_color);
            let edge_color = Color::new(0.5, 0.7, 1.0, 0.4);
            push_rect(&mut all_verts, map_x, vp_y, 2.0, vp_h, &edge_color);
        }
    }
    all_verts
}

/// Header/mode-line shadow depth effect.
pub(super) fn emit_header_shadow(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.header_shadow.enabled {
        return Vec::new();
    }
    let shadow_size = ctx.effects.header_shadow.size.max(1.0);
    let intensity = ctx.effects.header_shadow.intensity.clamp(0.0, 1.0);
    let steps = 8;
    let mut verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if info.is_minibuffer {
            continue;
        }
        let b = &info.bounds;

        // Detect header-line by checking overlay glyphs near window top
        let mut header_bottom: Option<f32> = None;
        for g in &ctx.frame_glyphs.glyphs {
            if let FrameGlyph::Char {
                x: _,
                y: _,
                height: _,
                row_role,
                ..
            }
            | FrameGlyph::Stretch {
                x: _,
                y: _,
                height: _,
                row_role,
                ..
            } = g
            {
                if !row_role.is_chrome() {
                    continue;
                }
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
                if gx >= b.x && gx < b.x + b.width && (gy - b.y).abs() < 2.0 {
                    let bottom = gy + gh;
                    header_bottom =
                        Some(header_bottom.map_or(bottom, |prev: f32| prev.max(bottom)));
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
                let color = Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: alpha,
                };
                push_rect(&mut verts, b.x, sy, b.width, strip_h, &color);
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
                let color = Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: alpha,
                };
                push_rect(&mut verts, b.x, sy, b.width, strip_h, &color);
            }
        }
    }
    verts
}

/// Active window border glow.
pub(super) fn emit_active_window_glow(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.window_glow.enabled {
        return Vec::new();
    }
    let glow_radius = ctx.effects.window_glow.radius.max(1.0);
    let intensity = ctx.effects.window_glow.intensity.clamp(0.0, 1.0);
    let (cr, cg, cb) = ctx.effects.window_glow.color;
    let steps = 10;
    let mut verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if !info.selected || info.is_minibuffer {
            continue;
        }
        let b = &info.bounds;

        for i in 0..steps {
            let t = (i + 1) as f32 / steps as f32;
            let alpha = intensity * (1.0 - t) * (1.0 - t);
            let offset = t * glow_radius;
            let strip_w = glow_radius / steps as f32;
            let color = Color::new(cr, cg, cb, alpha);

            // Top
            push_rect(
                &mut verts,
                b.x - offset,
                b.y - offset,
                b.width + offset * 2.0,
                strip_w,
                &color,
            );
            // Bottom
            push_rect(
                &mut verts,
                b.x - offset,
                b.y + b.height + offset - strip_w,
                b.width + offset * 2.0,
                strip_w,
                &color,
            );
            // Left
            push_rect(
                &mut verts,
                b.x - offset,
                b.y - offset,
                strip_w,
                b.height + offset * 2.0,
                &color,
            );
            // Right
            push_rect(
                &mut verts,
                b.x + b.width + offset - strip_w,
                b.y - offset,
                strip_w,
                b.height + offset * 2.0,
                &color,
            );
        }
    }
    verts
}

/// Scroll progress indicator bar at top of each window.
pub(super) fn emit_scroll_progress(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.scroll_progress.enabled {
        return Vec::new();
    }
    let bar_h = ctx.effects.scroll_progress.height.max(1.0);
    let (cr, cg, cb) = ctx.effects.scroll_progress.color;
    let opacity = ctx.effects.scroll_progress.opacity.clamp(0.0, 1.0);
    let mut verts: Vec<RectVertex> = Vec::new();

    for info in &ctx.frame_glyphs.window_infos {
        if info.is_minibuffer {
            continue;
        }
        let b = &info.bounds;
        let buf_size = info.buffer_size.max(1) as f32;

        let frac = (info.window_start as f32 / buf_size).clamp(0.0, 1.0);

        let visible =
            ((info.window_end - info.window_start).max(1) as f32 / buf_size).clamp(0.0, 1.0);

        // Track background
        let track_color = Color::new(cr, cg, cb, opacity * 0.15);
        push_rect(&mut verts, b.x, b.y, b.width, bar_h, &track_color);

        // Progress thumb
        let thumb_w = (visible * b.width).max(4.0);
        let thumb_x = b.x + frac * (b.width - thumb_w);
        let thumb_color = Color::new(cr, cg, cb, opacity);
        push_rect(&mut verts, thumb_x, b.y, thumb_w, bar_h, &thumb_color);
    }
    verts
}

/// Window content shadow/depth effect (inner shadow at edges).
pub(super) fn emit_window_content_shadow(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.window_content_shadow.enabled || ctx.frame_glyphs.window_infos.len() <= 1 {
        return Vec::new();
    }
    let shadow_size = ctx.effects.window_content_shadow.size.max(1.0);
    let shadow_opacity = ctx.effects.window_content_shadow.opacity.clamp(0.0, 1.0);
    let mut verts: Vec<RectVertex> = Vec::new();
    let steps = 4;

    for info in &ctx.frame_glyphs.window_infos {
        if info.is_minibuffer {
            continue;
        }
        let b = &info.bounds;

        for i in 0..steps {
            let frac = i as f32 / steps as f32;
            let alpha = shadow_opacity * (1.0 - frac) * (1.0 - frac);
            let thickness = shadow_size / steps as f32;
            let c = Color::new(0.0, 0.0, 0.0, alpha);

            // Top inner shadow
            push_rect(
                &mut verts,
                b.x,
                b.y + frac * shadow_size,
                b.width,
                thickness,
                &c,
            );
            // Left inner shadow
            push_rect(
                &mut verts,
                b.x + frac * shadow_size,
                b.y,
                thickness,
                b.height,
                &c,
            );
            // Right inner shadow
            push_rect(
                &mut verts,
                b.x + b.width - shadow_size + frac * shadow_size,
                b.y,
                thickness,
                b.height,
                &c,
            );
            // Bottom inner shadow (above mode-line)
            let content_bottom = b.y + b.height - info.mode_line_height;
            push_rect(
                &mut verts,
                b.x,
                content_bottom - shadow_size + frac * shadow_size,
                b.width,
                thickness,
                &c,
            );
        }
    }
    verts
}

/// Resize padding transition overlay.
/// `resize_padding_amount` is computed externally (from WgpuRenderer::resize_padding_amount()).
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_resize_padding(
    ctx: &EffectCtx,
    resize_padding_amount: f32,
) -> (Vec<RectVertex>, bool) {
    if resize_padding_amount <= 0.5 {
        return (Vec::new(), false);
    }
    let pad = resize_padding_amount;
    let bg = &ctx.frame_glyphs.background;
    let mut verts: Vec<RectVertex> = Vec::new();
    for info in &ctx.frame_glyphs.window_infos {
        let b = &info.bounds;
        // Top edge
        push_rect(&mut verts, b.x, b.y, b.width, pad, bg);
        // Bottom edge (above mode-line)
        let content_bottom = b.y + b.height - info.mode_line_height;
        push_rect(&mut verts, b.x, content_bottom - pad, b.width, pad, bg);
        // Left edge
        push_rect(
            &mut verts,
            b.x,
            b.y,
            pad,
            b.height - info.mode_line_height,
            bg,
        );
        // Right edge
        push_rect(
            &mut verts,
            b.x + b.width - pad,
            b.y,
            pad,
            b.height - info.mode_line_height,
            bg,
        );
    }
    (verts, true)
}

/// Mini-buffer completion highlight.
pub(super) fn emit_minibuffer_completion(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.minibuffer_highlight.enabled {
        return Vec::new();
    }
    let (hr, hg, hb) = ctx.effects.minibuffer_highlight.color;
    let h_opacity = ctx.effects.minibuffer_highlight.opacity.clamp(0.0, 1.0);
    let mut verts: Vec<RectVertex> = Vec::new();

    if let Some(mb_info) = ctx
        .frame_glyphs
        .window_infos
        .iter()
        .find(|w| w.is_minibuffer)
    {
        let mb = &mb_info.bounds;
        let mut highlighted_rows: Vec<(f32, f32, f32, f32)> = Vec::new();

        for glyph in &ctx.frame_glyphs.glyphs {
            // Every materialized Char carried `Some(face.background)`, so this
            // matched all char glyphs in the region; resolving by face is not
            // needed here — the row geometry alone drives the highlight.
            if let FrameGlyph::Char {
                x,
                y,
                width,
                height,
                ..
            } = glyph
                && *y >= mb.y
                && *y < mb.y + mb.height
                && *x >= mb.x
                && *x < mb.x + mb.width
            {
                let mut merged = false;
                for row in &mut highlighted_rows {
                    if (row.1 - *y).abs() < 1.0 {
                        row.0 = row.0.min(*x);
                        row.2 = row.2.max(*x + *width);
                        row.3 = row.3.max(*height);
                        merged = true;
                        break;
                    }
                }
                if !merged {
                    highlighted_rows.push((*x, *y, *x + *width, *height));
                }
            }
        }

        let glow_pad = 3.0_f32;
        for (x_min, y, x_max, height) in &highlighted_rows {
            let rx = (x_min - glow_pad).max(mb.x);
            let ry = (y - glow_pad).max(mb.y);
            let rw = (x_max - x_min + glow_pad * 2.0).min(mb.x + mb.width - rx);
            let rh = (height + glow_pad * 2.0).min(mb.y + mb.height - ry);

            for step in 0..3 {
                let s = step as f32;
                let alpha = h_opacity * (1.0 - s / 3.0) * (1.0 - s / 3.0);
                let c = Color::new(hr, hg, hb, alpha);
                push_rect(&mut verts, rx - s, ry - s, rw + s * 2.0, rh + s * 2.0, &c);
            }
        }
    }
    verts
}

/// Scroll velocity fade overlay.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_scroll_velocity_fade(
    ctx: &EffectCtx,
    scroll_velocity_fades: &mut Vec<ScrollVelocityFadeEntry>,
) -> (Vec<RectVertex>, bool) {
    if scroll_velocity_fades.is_empty() {
        return (Vec::new(), false);
    }
    let max_op = ctx.effects.scroll_velocity_fade.max_opacity.clamp(0.0, 1.0);
    let mut verts: Vec<RectVertex> = Vec::new();

    for entry in scroll_velocity_fades.iter() {
        let elapsed = ctx
            .frame_sample
            .since_at_presentation(entry.started)
            .as_millis() as f32;
        let duration = entry.duration.as_millis() as f32;
        if elapsed >= duration {
            continue;
        }

        let t = elapsed / duration;
        let fade = (1.0 - t) * (1.0 - t);
        let vel_factor = (entry.velocity / 50.0).min(1.0);
        let alpha = max_op * fade * vel_factor;
        if alpha < 0.005 {
            continue;
        }

        let b = &entry.bounds;
        let c = Color::new(0.0, 0.0, 0.0, alpha);
        push_rect(&mut verts, b.x, b.y, b.width, b.height, &c);
    }

    // Cleanup expired entries
    scroll_velocity_fades
        .retain(|e| ctx.frame_sample.since_at_presentation(e.started) < e.duration);
    let needs_redraw = !scroll_velocity_fades.is_empty();
    (verts, needs_redraw)
}

/// Click halo effect.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_click_halo(
    ctx: &EffectCtx,
    click_halos: &mut Vec<ClickHaloEntry>,
) -> (Vec<RectVertex>, bool) {
    if click_halos.is_empty() {
        return (Vec::new(), false);
    }
    let (hr, hg, hb) = ctx.effects.click_halo.color;
    let max_r = ctx.effects.click_halo.max_radius;
    let mut verts: Vec<RectVertex> = Vec::new();
    let ring_steps = 8;

    for entry in click_halos.iter() {
        let elapsed = ctx
            .frame_sample
            .since_at_presentation(entry.started)
            .as_millis() as f32;
        let duration = entry.duration.as_millis() as f32;
        if elapsed >= duration {
            continue;
        }

        let t = elapsed / duration;
        let radius = max_r * t;
        let alpha = 0.4 * (1.0 - t) * (1.0 - t);
        if alpha < 0.005 {
            continue;
        }

        let ring_w = 2.0_f32;
        for step in 0..ring_steps {
            let angle = step as f32 * std::f32::consts::TAU / ring_steps as f32;
            let px = entry.x + radius * angle.cos();
            let py = entry.y + radius * angle.sin();
            let c = Color::new(hr, hg, hb, alpha);
            push_rect(
                &mut verts,
                px - ring_w * 0.5,
                py - ring_w * 0.5,
                ring_w,
                ring_w,
                &c,
            );
        }
    }

    click_halos.retain(|e| ctx.frame_sample.since_at_presentation(e.started) < e.duration);
    let needs_redraw = !click_halos.is_empty();
    (verts, needs_redraw)
}

/// Window edge snap indicator.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_edge_snap(
    ctx: &EffectCtx,
    edge_snaps: &mut Vec<EdgeSnapEntry>,
) -> (Vec<RectVertex>, bool) {
    if edge_snaps.is_empty() {
        return (Vec::new(), false);
    }
    let (er, eg, eb) = ctx.effects.edge_snap.color;
    let mut verts: Vec<RectVertex> = Vec::new();
    let bar_h = 4.0_f32;
    let steps = 3;

    for entry in edge_snaps.iter() {
        let elapsed = ctx
            .frame_sample
            .since_at_presentation(entry.started)
            .as_millis() as f32;
        let duration = entry.duration.as_millis() as f32;
        if elapsed >= duration {
            continue;
        }

        let t = elapsed / duration;
        let alpha = 0.5 * (1.0 - t) * (1.0 - t);
        if alpha < 0.005 {
            continue;
        }

        let b = &entry.bounds;

        if entry.at_top {
            for i in 0..steps {
                let frac = i as f32 / steps as f32;
                let a = alpha * (1.0 - frac);
                let c = Color::new(er, eg, eb, a);
                push_rect(
                    &mut verts,
                    b.x,
                    b.y + frac * bar_h,
                    b.width,
                    bar_h / steps as f32,
                    &c,
                );
            }
        }
        if entry.at_bottom {
            let content_bottom = b.y + b.height - entry.mode_line_height;
            for i in 0..steps {
                let frac = i as f32 / steps as f32;
                let a = alpha * (1.0 - frac);
                let c = Color::new(er, eg, eb, a);
                push_rect(
                    &mut verts,
                    b.x,
                    content_bottom - bar_h + frac * bar_h,
                    b.width,
                    bar_h / steps as f32,
                    &c,
                );
            }
        }
    }

    edge_snaps.retain(|e| ctx.frame_sample.since_at_presentation(e.started) < e.duration);
    let needs_redraw = !edge_snaps.is_empty();
    (verts, needs_redraw)
}

/// Line wrap indicator overlay.
pub(super) fn emit_line_wrap_indicator(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.wrap_indicator.enabled {
        return Vec::new();
    }
    let (wr, wg, wb) = ctx.effects.wrap_indicator.color;
    let w_opacity = ctx.effects.wrap_indicator.opacity.clamp(0.0, 1.0);
    let mut verts: Vec<RectVertex> = Vec::new();

    for glyph in &ctx.frame_glyphs.glyphs {
        if let FrameGlyph::Char {
            char: ch,
            x,
            y,
            height,
            ..
        } = glyph
            && *ch == '\u{21B5}'
        {
            for info in &ctx.frame_glyphs.window_infos {
                if info.is_minibuffer {
                    continue;
                }
                let b = &info.bounds;
                let text_bottom = b.y + b.height - info.mode_line_height;
                if *y >= b.y && *y < text_bottom && *x >= b.x && *x < b.x + b.width {
                    let grad_w = 20.0_f32.min(b.width * 0.1);
                    let text_right = *x;
                    let grad_x = text_right - grad_w;
                    let draw_h = (text_bottom - *y).min(*height).max(0.0);
                    if draw_h <= 0.0 {
                        break;
                    }
                    let steps = 5;
                    for i in 0..steps {
                        let frac = i as f32 / steps as f32;
                        let step_alpha = w_opacity * frac * frac;
                        let sx = grad_x + frac * grad_w;
                        let sw = grad_w / steps as f32;
                        let c = Color::new(wr, wg, wb, step_alpha);
                        push_rect(&mut verts, sx, *y, sw, draw_h, &c);
                    }
                    break;
                }
            }
        }
    }
    verts
}

/// Scroll momentum indicator.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_scroll_momentum(
    ctx: &EffectCtx,
    active_scroll_momentums: &[ScrollMomentumEntry],
) -> (Vec<RectVertex>, bool) {
    if active_scroll_momentums.is_empty() {
        return (Vec::new(), false);
    }
    let bar_w = ctx.effects.scroll_momentum.width.max(1.0);
    let mut verts: Vec<RectVertex> = Vec::new();
    let now = ctx.frame_sample.presentation_time();

    for entry in active_scroll_momentums {
        let elapsed = now.saturating_since(entry.started);
        if elapsed >= entry.duration {
            continue;
        }
        let t = elapsed.as_secs_f32() / entry.duration.as_secs_f32();
        let alpha = (1.0 - t) * (1.0 - t);
        let b = &entry.bounds;
        let content_h = b.height;

        let bar_x = b.x + b.width - bar_w - 2.0;
        if entry.direction > 0 {
            let arrow_h = (content_h * 0.15).min(40.0);
            let steps = 8;
            for i in 0..steps {
                let frac = i as f32 / steps as f32;
                let step_alpha = alpha * (1.0 - frac) * 0.6;
                let sy = b.y + content_h - arrow_h + frac * arrow_h;
                let sh = arrow_h / steps as f32;
                let c = Color::new(0.5, 0.7, 1.0, step_alpha);
                push_rect(&mut verts, bar_x, sy, bar_w, sh, &c);
            }
        } else {
            let arrow_h = (content_h * 0.15).min(40.0);
            let steps = 8;
            for i in 0..steps {
                let frac = i as f32 / steps as f32;
                let step_alpha = alpha * (1.0 - frac) * 0.6;
                let sy = b.y + frac * arrow_h;
                let sh = arrow_h / steps as f32;
                let c = Color::new(0.5, 0.7, 1.0, step_alpha);
                push_rect(&mut verts, bar_x, sy, bar_w, sh, &c);
            }
        }
    }
    (verts, true)
}

/// Vignette effect: darken edges of the frame.
pub(super) fn emit_vignette(ctx: &EffectCtx) -> Vec<RectVertex> {
    if !ctx.effects.vignette.enabled {
        return Vec::new();
    }
    let frame_w = ctx.frame_glyphs.width;
    let frame_h = ctx.frame_glyphs.height;
    let intensity = ctx.effects.vignette.intensity.clamp(0.0, 1.0);
    let radius_pct = ctx.effects.vignette.radius.clamp(10.0, 100.0) / 100.0;

    let steps = 16;
    let mut verts: Vec<RectVertex> = Vec::new();

    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let alpha = intensity * t * t;
        let inset = (1.0 - t) * radius_pct * frame_w.min(frame_h) * 0.5;
        let strip_w = inset / steps as f32;

        if strip_w < 0.5 {
            continue;
        }

        let color = Color::new(0.0, 0.0, 0.0, alpha);

        // Top edge strip
        let y = inset - strip_w;
        if y >= 0.0 {
            push_rect(&mut verts, 0.0, y, frame_w, strip_w, &color);
        }
        // Bottom edge strip
        let by = frame_h - inset;
        if by < frame_h {
            push_rect(&mut verts, 0.0, by, frame_w, strip_w, &color);
        }
        // Left edge strip
        let lx = inset - strip_w;
        if lx >= 0.0 {
            push_rect(&mut verts, lx, 0.0, strip_w, frame_h, &color);
        }
        // Right edge strip
        let rx = frame_w - inset;
        if rx < frame_w {
            push_rect(&mut verts, rx, 0.0, strip_w, frame_h, &color);
        }
    }
    verts
}

/// Window switch highlight fade.
/// Returns (vertices, needs_continuous_redraw).
pub(super) fn emit_window_switch_fade(
    ctx: &EffectCtx,
    active_window_fades: &mut Vec<WindowFadeEntry>,
) -> (Vec<RectVertex>, bool) {
    if active_window_fades.is_empty() {
        return (Vec::new(), false);
    }
    let mut verts: Vec<RectVertex> = Vec::new();
    let now = ctx.frame_sample.presentation_time();

    for fade in active_window_fades.iter() {
        let elapsed = now.saturating_since(fade.started);
        let t = (elapsed.as_secs_f32() / fade.duration.as_secs_f32()).min(1.0);
        if t >= 1.0 {
            continue;
        }
        let eased = t * (2.0 - t);
        let alpha = fade.intensity * (1.0 - eased);

        let color = Color::new(1.0, 1.0, 1.0, alpha);
        push_rect(
            &mut verts,
            fade.bounds.x,
            fade.bounds.y,
            fade.bounds.width,
            fade.bounds.height,
            &color,
        );
    }

    // Clean up completed fades
    active_window_fades.retain(|f| {
        ctx.frame_sample
            .since_at_presentation(f.started)
            .as_secs_f32()
            < f.duration.as_secs_f32()
    });
    let needs_redraw = !active_window_fades.is_empty();
    (verts, needs_redraw)
}

// ===========================================================================
// Unit Tests
// ===========================================================================

#[cfg(test)]
#[path = "window_effects_test.rs"]
mod tests;
