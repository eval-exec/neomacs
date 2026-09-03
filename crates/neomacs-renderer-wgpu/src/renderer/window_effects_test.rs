use super::*;
use crate::clock::Instant;
use crate::effect_config::EffectsConfig;
use neomacs_display_protocol::frame_glyphs::{FrameGlyphBuffer, WindowInfo};
use neomacs_display_protocol::types::Rect;

/// Helper to create a test EffectCtx
fn test_ctx<'a>(effects: &'a EffectsConfig, frame_glyphs: &'a FrameGlyphBuffer) -> EffectCtx<'a> {
    EffectCtx {
        effects,
        frame_glyphs,
        animated_cursor: &None,
        cursor_visible: false,
        mouse_pos: (0.0, 0.0),
        surface_width: 800,
        surface_height: 600,
        aurora_start: Instant::now(),
        scale_factor: 1.0,
        logical_w: 800.0,
        logical_h: 600.0,
        renderer_width: 800.0,
        renderer_height: 600.0,
    }
}

/// Helper to create a test WindowInfo
fn test_window_info(
    window_id: i64,
    bounds: Rect,
    selected: bool,
    is_minibuffer: bool,
    modified: bool,
    mode_line_height: f32,
) -> WindowInfo {
    WindowInfo {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(window_id),
        buffer_id: 1,
        window_start: 0,
        window_end: 100,
        buffer_size: 100,
        bounds,
        geometry: Default::default(),
        mode_line_height,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected,
        is_minibuffer,
        char_height: 20.0,
        buffer_name: String::new(),
        buffer_file_name: String::new(),
        modified,
    }
}

// ========================================================================
// extension_to_color tests
// ========================================================================

#[test]
fn test_extension_to_color_known_extensions() {
    // Test all known extensions
    assert_eq!(extension_to_color("rs"), (0.8, 0.3, 0.1));
    assert_eq!(extension_to_color("el"), (0.6, 0.2, 0.8));
    assert_eq!(extension_to_color("lisp"), (0.6, 0.2, 0.8));
    assert_eq!(extension_to_color("scm"), (0.6, 0.2, 0.8));
    assert_eq!(extension_to_color("c"), (0.2, 0.5, 0.8));
    assert_eq!(extension_to_color("h"), (0.2, 0.5, 0.8));
    assert_eq!(extension_to_color("cpp"), (0.2, 0.4, 0.7));
    assert_eq!(extension_to_color("cc"), (0.2, 0.4, 0.7));
    assert_eq!(extension_to_color("hpp"), (0.2, 0.4, 0.7));
    assert_eq!(extension_to_color("py"), (0.2, 0.6, 0.2));
    assert_eq!(extension_to_color("js"), (0.9, 0.8, 0.2));
    assert_eq!(extension_to_color("jsx"), (0.9, 0.8, 0.2));
    assert_eq!(extension_to_color("ts"), (0.2, 0.5, 0.9));
    assert_eq!(extension_to_color("tsx"), (0.2, 0.5, 0.9));
    assert_eq!(extension_to_color("rb"), (0.8, 0.2, 0.2));
    assert_eq!(extension_to_color("go"), (0.0, 0.6, 0.7));
    assert_eq!(extension_to_color("java"), (0.7, 0.3, 0.1));
    assert_eq!(extension_to_color("html"), (0.9, 0.3, 0.2));
    assert_eq!(extension_to_color("htm"), (0.9, 0.3, 0.2));
    assert_eq!(extension_to_color("css"), (0.2, 0.4, 0.9));
    assert_eq!(extension_to_color("scss"), (0.2, 0.4, 0.9));
    assert_eq!(extension_to_color("json"), (0.5, 0.5, 0.5));
    assert_eq!(extension_to_color("yaml"), (0.5, 0.5, 0.5));
    assert_eq!(extension_to_color("yml"), (0.5, 0.5, 0.5));
    assert_eq!(extension_to_color("toml"), (0.5, 0.5, 0.5));
    assert_eq!(extension_to_color("md"), (0.4, 0.7, 0.4));
    assert_eq!(extension_to_color("org"), (0.4, 0.7, 0.4));
    assert_eq!(extension_to_color("txt"), (0.4, 0.7, 0.4));
    assert_eq!(extension_to_color("sh"), (0.3, 0.7, 0.3));
    assert_eq!(extension_to_color("bash"), (0.3, 0.7, 0.3));
    assert_eq!(extension_to_color("zsh"), (0.3, 0.7, 0.3));
}

#[test]
fn test_extension_to_color_unknown_extensions() {
    // Test unknown extensions produce valid RGB values
    let (r1, g1, b1) = extension_to_color("xyz");
    assert!(r1 >= 0.0 && r1 <= 1.0);
    assert!(g1 >= 0.0 && g1 <= 1.0);
    assert!(b1 >= 0.0 && b1 <= 1.0);

    let (r2, g2, b2) = extension_to_color("unknown");
    assert!(r2 >= 0.0 && r2 <= 1.0);
    assert!(g2 >= 0.0 && g2 <= 1.0);
    assert!(b2 >= 0.0 && b2 <= 1.0);
}

#[test]
fn test_extension_to_color_deterministic() {
    // Same extension should produce same color
    let color1 = extension_to_color("abc");
    let color2 = extension_to_color("abc");
    assert_eq!(color1, color2);

    // Different extensions should produce different colors
    let color3 = extension_to_color("def");
    assert_ne!(color1, color3);
}

#[test]
fn test_extension_to_color_empty_string() {
    // Empty string should produce valid color
    let (r, g, b) = extension_to_color("");
    assert!(r >= 0.0 && r <= 1.0);
    assert!(g >= 0.0 && g <= 1.0);
    assert!(b >= 0.0 && b <= 1.0);
}

// ========================================================================
// emit_modified_indicator tests
// ========================================================================

#[test]
fn test_emit_modified_indicator_disabled() {
    let mut effects = EffectsConfig::default();
    effects.modified_indicator.enabled = false;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let verts = emit_modified_indicator(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_modified_indicator_no_windows() {
    let mut effects = EffectsConfig::default();
    effects.modified_indicator.enabled = true;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let verts = emit_modified_indicator(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_modified_indicator_modified_window() {
    let mut effects = EffectsConfig::default();
    effects.modified_indicator.enabled = true;
    effects.modified_indicator.width = 3.0;
    effects.modified_indicator.color = (1.0, 0.6, 0.2);
    effects.modified_indicator.opacity = 0.8;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, false, true, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_modified_indicator(&ctx);

    // Should produce vertices (exact count depends on implementation)
    assert!(verts.len() > 0);
}

#[test]
fn test_emit_modified_indicator_unmodified_window() {
    let mut effects = EffectsConfig::default();
    effects.modified_indicator.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, false, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_modified_indicator(&ctx);

    // Unmodified window should not produce vertices
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_modified_indicator_minibuffer() {
    let mut effects = EffectsConfig::default();
    effects.modified_indicator.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, true, true, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_modified_indicator(&ctx);

    // Minibuffer should not show indicator
    assert_eq!(verts.len(), 0);
}

// ========================================================================
// emit_stained_glass tests
// ========================================================================

#[test]
fn test_emit_stained_glass_disabled() {
    let mut effects = EffectsConfig::default();
    effects.stained_glass.enabled = false;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let verts = emit_stained_glass(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_stained_glass_enabled_with_window() {
    let mut effects = EffectsConfig::default();
    effects.stained_glass.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    // Non-selected, non-minibuffer window should get stained glass
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, false, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_stained_glass(&ctx);

    // Should produce vertices
    assert!(verts.len() > 0);
}

#[test]
fn test_emit_stained_glass_selected_window() {
    let mut effects = EffectsConfig::default();
    effects.stained_glass.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    // Selected window should not get stained glass
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, true, false, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_stained_glass(&ctx);

    // Selected window should not produce vertices
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_stained_glass_minibuffer() {
    let mut effects = EffectsConfig::default();
    effects.stained_glass.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    // Minibuffer should not get stained glass
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, true, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_stained_glass(&ctx);

    // Minibuffer should not produce vertices
    assert_eq!(verts.len(), 0);
}

// ========================================================================
// emit_focus_gradient_border tests
// ========================================================================

#[test]
fn test_emit_focus_gradient_border_disabled() {
    let mut effects = EffectsConfig::default();
    effects.focus_gradient_border.enabled = false;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let verts = emit_focus_gradient_border(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_focus_gradient_border_enabled_no_selected() {
    let mut effects = EffectsConfig::default();
    effects.focus_gradient_border.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    // Not selected
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, false, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_focus_gradient_border(&ctx);

    // Non-selected window should not produce vertices
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_focus_gradient_border_selected() {
    let mut effects = EffectsConfig::default();
    effects.focus_gradient_border.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    // Selected, non-minibuffer
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, true, false, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_focus_gradient_border(&ctx);

    // Selected window should produce vertices
    assert!(verts.len() > 0);
}

// ========================================================================
// emit_window_depth_shadow tests
// ========================================================================

#[test]
fn test_emit_window_depth_shadow_disabled() {
    let mut effects = EffectsConfig::default();
    effects.depth_shadow.enabled = false;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let verts = emit_window_depth_shadow(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_window_depth_shadow_enabled() {
    let mut effects = EffectsConfig::default();
    effects.depth_shadow.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, false, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_window_depth_shadow(&ctx);

    // Should produce shadow vertices
    assert!(verts.len() > 0);
}

#[test]
fn test_emit_window_depth_shadow_minibuffer() {
    let mut effects = EffectsConfig::default();
    effects.depth_shadow.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    // Minibuffer should not get shadow
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, true, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_window_depth_shadow(&ctx);

    // Minibuffer should not produce vertices
    assert_eq!(verts.len(), 0);
}

// ========================================================================
// emit_mode_line_gradient tests
// ========================================================================

#[test]
fn test_emit_mode_line_gradient_disabled() {
    let mut effects = EffectsConfig::default();
    effects.mode_line_gradient.enabled = false;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let verts = emit_mode_line_gradient(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_mode_line_gradient_no_mode_line() {
    let mut effects = EffectsConfig::default();
    effects.mode_line_gradient.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    // mode_line_height = 0 (no mode line)
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, false, false, 0.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_mode_line_gradient(&ctx);

    // No mode line, no vertices
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_mode_line_gradient_with_mode_line() {
    let mut effects = EffectsConfig::default();
    effects.mode_line_gradient.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    // mode_line_height = 20.0
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, false, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_mode_line_gradient(&ctx);

    // Should produce gradient vertices
    assert!(verts.len() > 0);
}

#[test]
fn test_emit_mode_line_gradient_minibuffer() {
    let mut effects = EffectsConfig::default();
    effects.mode_line_gradient.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    // Minibuffer should not get mode line gradient
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, true, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_mode_line_gradient(&ctx);

    // Minibuffer should not produce vertices
    assert_eq!(verts.len(), 0);
}

// ========================================================================
// emit_window_corner_fold tests
// ========================================================================

#[test]
fn test_emit_window_corner_fold_disabled() {
    let mut effects = EffectsConfig::default();
    effects.corner_fold.enabled = false;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let verts = emit_window_corner_fold(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_window_corner_fold_enabled() {
    let mut effects = EffectsConfig::default();
    effects.corner_fold.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, false, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_window_corner_fold(&ctx);

    // Should produce corner fold vertices
    assert!(verts.len() > 0);
}

#[test]
fn test_emit_window_corner_fold_minibuffer() {
    let mut effects = EffectsConfig::default();
    effects.corner_fold.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, true, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_window_corner_fold(&ctx);

    // Minibuffer should not produce vertices
    assert_eq!(verts.len(), 0);
}

// ========================================================================
// emit_frosted_window_border tests
// ========================================================================

#[test]
fn test_emit_frosted_window_border_disabled() {
    let mut effects = EffectsConfig::default();
    effects.frosted_border.enabled = false;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let verts = emit_frosted_window_border(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_frosted_window_border_enabled() {
    let mut effects = EffectsConfig::default();
    effects.frosted_border.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, false, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_frosted_window_border(&ctx);

    // Should produce border vertices
    assert!(verts.len() > 0);
}

#[test]
fn test_emit_frosted_window_border_minibuffer() {
    let mut effects = EffectsConfig::default();
    effects.frosted_border.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, true, false, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);
    let verts = emit_frosted_window_border(&ctx);

    // Minibuffer should not produce vertices
    assert_eq!(verts.len(), 0);
}

// ========================================================================
// emit_window_scanline tests
// ========================================================================

#[test]
fn test_emit_window_scanline_disabled() {
    let mut effects = EffectsConfig::default();
    effects.scanlines.enabled = false;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let verts = emit_window_scanline(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_emit_window_scanline_enabled() {
    let mut effects = EffectsConfig::default();
    effects.scanlines.enabled = true;
    effects.scanlines.spacing = 4;

    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let verts = emit_window_scanline(&ctx);

    // Should produce scanline vertices (independent of windows)
    assert!(verts.len() > 0);

    // Should have vertices at regular intervals
    // With spacing=4 and height=600, we expect 150 scanlines
    let expected_scanlines = (600.0_f32 / 4.0_f32).ceil() as usize;
    // Each scanline is one rect (6 vertices)
    assert_eq!(verts.len(), expected_scanlines * 6);
}

// ========================================================================
// emit_cursor_ghost tests
// ========================================================================

#[test]
fn test_emit_cursor_ghost_disabled() {
    let mut effects = EffectsConfig::default();
    effects.cursor_ghost.enabled = false;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let mut ghost_entries = Vec::new();
    let (verts, needs_redraw) = emit_cursor_ghost(&ctx, &mut ghost_entries);

    assert_eq!(verts.len(), 0);
    assert_eq!(needs_redraw, false);
}

#[test]
fn test_emit_cursor_ghost_enabled_no_entries() {
    let mut effects = EffectsConfig::default();
    effects.cursor_ghost.enabled = true;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let mut ghost_entries = Vec::new();
    let (verts, needs_redraw) = emit_cursor_ghost(&ctx, &mut ghost_entries);

    // No ghost entries, so no vertices
    assert_eq!(verts.len(), 0);
    assert_eq!(needs_redraw, false);
}

// ========================================================================
// emit_edge_glow tests
// ========================================================================

#[test]
fn test_emit_edge_glow_disabled() {
    let mut effects = EffectsConfig::default();
    effects.edge_glow.enabled = false;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let mut edge_glow_entries = Vec::new();
    let (verts, needs_redraw) = emit_edge_glow(&ctx, &mut edge_glow_entries);

    assert_eq!(verts.len(), 0);
    assert_eq!(needs_redraw, false);
}

#[test]
fn test_emit_edge_glow_enabled_no_entries() {
    let mut effects = EffectsConfig::default();
    effects.edge_glow.enabled = true;
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    let mut edge_glow_entries = Vec::new();
    let (verts, needs_redraw) = emit_edge_glow(&ctx, &mut edge_glow_entries);

    // No glow entries, so no vertices
    assert_eq!(verts.len(), 0);
    assert_eq!(needs_redraw, false);
}

// ========================================================================
// Integration test: multiple effects together
// ========================================================================

#[test]
fn test_multiple_effects_together() {
    let mut effects = EffectsConfig::default();
    effects.modified_indicator.enabled = true;
    effects.mode_line_gradient.enabled = true;
    effects.scanlines.enabled = true;

    let mut frame_glyphs = FrameGlyphBuffer::new();
    let bounds = Rect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 200.0,
    };
    frame_glyphs
        .window_infos
        .push(test_window_info(1, bounds, false, false, true, 20.0));

    let ctx = test_ctx(&effects, &frame_glyphs);

    // Each effect should produce vertices independently
    let mod_verts = emit_modified_indicator(&ctx);
    assert!(mod_verts.len() > 0);

    let ml_verts = emit_mode_line_gradient(&ctx);
    assert!(ml_verts.len() > 0);

    let scan_verts = emit_window_scanline(&ctx);
    assert!(scan_verts.len() > 0);
}

#[test]
fn test_empty_frame_glyphs() {
    let effects = EffectsConfig::default();
    let frame_glyphs = FrameGlyphBuffer::new();
    let ctx = test_ctx(&effects, &frame_glyphs);

    // All effects should handle empty frame_glyphs gracefully
    assert_eq!(emit_modified_indicator(&ctx).len(), 0);
    assert_eq!(emit_stained_glass(&ctx).len(), 0);
    assert_eq!(emit_focus_gradient_border(&ctx).len(), 0);
    assert_eq!(emit_window_depth_shadow(&ctx).len(), 0);
    assert_eq!(emit_mode_line_gradient(&ctx).len(), 0);
    assert_eq!(emit_window_corner_fold(&ctx).len(), 0);
    assert_eq!(emit_frosted_window_border(&ctx).len(), 0);
}
