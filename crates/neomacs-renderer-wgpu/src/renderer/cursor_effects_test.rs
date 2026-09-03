use super::super::effect_common::EffectCtx;
use super::*;
use crate::effect_config::EffectsConfig;
use neomacs_display_protocol::frame_glyphs::{FrameGlyphBuffer, WindowInfo};
use neomacs_display_protocol::types::{AnimatedCursor, Rect};

/// Helper to create an EffectCtx for testing
fn make_ctx<'a>(
    effects: &'a EffectsConfig,
    fgb: &'a FrameGlyphBuffer,
    animated_cursor: &'a Option<AnimatedCursor>,
    cursor_visible: bool,
) -> EffectCtx<'a> {
    EffectCtx {
        effects,
        frame_glyphs: fgb,
        animated_cursor,
        cursor_visible,
        mouse_pos: (400.0, 300.0),
        surface_width: 800,
        surface_height: 600,
        aurora_start: crate::clock::Instant::now(),
        scale_factor: 1.0,
        logical_w: 800.0,
        logical_h: 600.0,
        renderer_width: 800.0,
        renderer_height: 600.0,
    }
}

/// Helper to create a test animated cursor
fn make_animated_cursor(x: f32, y: f32, w: f32, h: f32, window_id: i64) -> AnimatedCursor {
    AnimatedCursor {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(window_id),
        x,
        y,
        width: w,
        height: h,
        corners: None,
        frame_id: neomacs_display_protocol::types::DisplayFrameId::new(0),
    }
}

/// Helper to create a selected WindowInfo for testing
fn make_selected_window_info(x: f32, y: f32, w: f32, h: f32) -> WindowInfo {
    WindowInfo {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(1),
        buffer_id: 1,
        window_start: 0,
        window_end: 100,
        buffer_size: 200,
        bounds: Rect::new(x, y, w, h),
        geometry: Default::default(),
        mode_line_height: 20.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: true,
        is_minibuffer: false,
        char_height: 16.0,
        buffer_name: String::new(),
        buffer_file_name: String::new(),
        modified: false,
    }
}

/// Property test: all vertices have valid positions and colors
fn validate_vertices(vertices: &[RectVertex]) {
    for (i, v) in vertices.iter().enumerate() {
        assert!(
            v.position[0].is_finite(),
            "vertex {} x position not finite",
            i
        );
        assert!(
            v.position[1].is_finite(),
            "vertex {} y position not finite",
            i
        );
        assert!(
            v.color[0].is_finite() && v.color[0] >= 0.0 && v.color[0] <= 1.0,
            "vertex {} r invalid",
            i
        );
        assert!(
            v.color[1].is_finite() && v.color[1] >= 0.0 && v.color[1] <= 1.0,
            "vertex {} g invalid",
            i
        );
        assert!(
            v.color[2].is_finite() && v.color[2] >= 0.0 && v.color[2] <= 1.0,
            "vertex {} b invalid",
            i
        );
        assert!(
            v.color[3].is_finite() && v.color[3] >= 0.0 && v.color[3] <= 1.0,
            "vertex {} a invalid",
            i
        );
    }
}

/// Property test: vertex count is always multiple of 6 (each rect = 2 triangles = 6 verts)
fn validate_vertex_count(vertices: &[RectVertex]) {
    assert_eq!(
        vertices.len() % 6,
        0,
        "vertex count {} not multiple of 6",
        vertices.len()
    );
}

// ========================================================================
// emit_cursor_glow tests
// ========================================================================

#[test]
fn test_cursor_glow_disabled() {
    let mut config = EffectsConfig::default();
    config.cursor_glow.enabled = false;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);
    let cursor_pulse_start = crate::clock::Instant::now();

    let verts = emit_cursor_glow(&ctx, &cursor_pulse_start);
    assert_eq!(verts.len(), 0, "disabled glow should produce no vertices");
}

#[test]
fn test_cursor_glow_cursor_not_visible() {
    let mut config = EffectsConfig::default();
    config.cursor_glow.enabled = true;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, false);
    let cursor_pulse_start = crate::clock::Instant::now();

    let verts = emit_cursor_glow(&ctx, &cursor_pulse_start);
    assert_eq!(verts.len(), 0, "invisible cursor should produce no glow");
}

#[test]
fn test_cursor_glow_no_cursor() {
    let mut config = EffectsConfig::default();
    config.cursor_glow.enabled = true;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = None;

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);
    let cursor_pulse_start = crate::clock::Instant::now();

    let verts = emit_cursor_glow(&ctx, &cursor_pulse_start);
    assert_eq!(verts.len(), 0, "no cursor should produce no glow");
}

#[test]
fn test_cursor_glow_with_animated_cursor() {
    let mut config = EffectsConfig::default();
    config.cursor_glow.enabled = true;
    config.cursor_glow.radius = 30.0;
    config.cursor_glow.opacity = 0.5;
    config.cursor_glow.color = (1.0, 0.5, 0.25);

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);
    let cursor_pulse_start = crate::clock::Instant::now();

    let verts = emit_cursor_glow(&ctx, &cursor_pulse_start);

    assert!(verts.len() > 0, "glow should produce vertices");
    validate_vertex_count(&verts);
    validate_vertices(&verts);

    // With radius 30.0, layers = ceil(30.0/2.0) = 15 layers, each is 6 verts
    let expected_layers = (30.0_f32 / 2.0).ceil() as usize;
    assert_eq!(verts.len(), expected_layers * 6);
}

#[test]
fn test_cursor_glow_with_pulse() {
    let mut config = EffectsConfig::default();
    config.cursor_glow.enabled = true;
    config.cursor_glow.radius = 20.0;
    config.cursor_pulse.enabled = true;
    config.cursor_pulse.speed = 1.0;
    config.cursor_pulse.min_opacity = 0.3;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);
    let cursor_pulse_start = crate::clock::Instant::now();

    let verts = emit_cursor_glow(&ctx, &cursor_pulse_start);

    assert!(verts.len() > 0, "glow with pulse should produce vertices");
    validate_vertex_count(&verts);
    validate_vertices(&verts);
}

// ========================================================================
// emit_cursor_crosshair tests
// ========================================================================

#[test]
fn test_cursor_crosshair_disabled() {
    let mut config = EffectsConfig::default();
    config.cursor_crosshair.enabled = false;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    let verts = emit_cursor_crosshair(&ctx);
    assert_eq!(
        verts.len(),
        0,
        "disabled crosshair should produce no vertices"
    );
}

#[test]
fn test_cursor_crosshair_cursor_not_visible() {
    let mut config = EffectsConfig::default();
    config.cursor_crosshair.enabled = true;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, false);

    let verts = emit_cursor_crosshair(&ctx);
    assert_eq!(
        verts.len(),
        0,
        "invisible cursor should produce no crosshair"
    );
}

#[test]
fn test_cursor_crosshair_no_cursor() {
    let mut config = EffectsConfig::default();
    config.cursor_crosshair.enabled = true;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = None;

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    let verts = emit_cursor_crosshair(&ctx);
    assert_eq!(verts.len(), 0, "no cursor should produce no crosshair");
}

#[test]
fn test_cursor_crosshair_no_selected_window() {
    let mut config = EffectsConfig::default();
    config.cursor_crosshair.enabled = true;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    let verts = emit_cursor_crosshair(&ctx);
    assert_eq!(
        verts.len(),
        0,
        "no selected window should produce no crosshair"
    );
}

#[test]
fn test_cursor_crosshair_with_selected_window() {
    let mut config = EffectsConfig::default();
    config.cursor_crosshair.enabled = true;
    config.cursor_crosshair.opacity = 0.5;
    config.cursor_crosshair.color = (0.5, 0.5, 0.5);

    let mut fgb = FrameGlyphBuffer::default();
    fgb.window_infos
        .push(make_selected_window_info(0.0, 0.0, 800.0, 600.0));

    let anim_cursor = Some(make_animated_cursor(400.0, 300.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    let verts = emit_cursor_crosshair(&ctx);

    // Should produce 2 rects: horizontal line + vertical line = 12 vertices
    assert_eq!(
        verts.len(),
        12,
        "crosshair should produce 2 rects (12 vertices)"
    );
    validate_vertex_count(&verts);
    validate_vertices(&verts);
}

#[test]
fn test_cursor_crosshair_minibuffer_excluded() {
    let mut config = EffectsConfig::default();
    config.cursor_crosshair.enabled = true;

    let mut fgb = FrameGlyphBuffer::default();
    let mut win_info = make_selected_window_info(0.0, 0.0, 800.0, 600.0);
    win_info.is_minibuffer = true;
    fgb.window_infos.push(win_info);

    let anim_cursor = Some(make_animated_cursor(400.0, 300.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    let verts = emit_cursor_crosshair(&ctx);
    assert_eq!(verts.len(), 0, "minibuffer should not show crosshair");
}

// ========================================================================
// emit_cursor_magnetism tests
// ========================================================================

#[test]
fn test_cursor_magnetism_disabled() {
    let mut config = EffectsConfig::default();
    config.cursor_magnetism.enabled = false;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);
    let mut entries = Vec::new();

    let (verts, needs_redraw) = emit_cursor_magnetism(&ctx, &mut entries);
    assert_eq!(verts.len(), 0);
    assert_eq!(needs_redraw, false);
}

#[test]
fn test_cursor_magnetism_no_entries() {
    let mut config = EffectsConfig::default();
    config.cursor_magnetism.enabled = true;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);
    let mut entries = Vec::new();

    let (verts, needs_redraw) = emit_cursor_magnetism(&ctx, &mut entries);

    // First call should create initial entry
    assert_eq!(entries.len(), 1);
    assert!(verts.len() > 0, "magnetism should produce vertices");
    assert_eq!(needs_redraw, true);
    validate_vertex_count(&verts);
    validate_vertices(&verts);
}

#[test]
fn test_cursor_magnetism_expired_entries_pruned() {
    let mut config = EffectsConfig::default();
    config.cursor_magnetism.enabled = true;
    config.cursor_magnetism.duration_ms = 1; // Very short duration

    let fgb = FrameGlyphBuffer::default();
    // Cursor at a different position to trigger a jump and add new entry
    let anim_cursor = Some(make_animated_cursor(200.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    // Add an old entry at a different position
    let old_time = crate::clock::Instant::now() - std::time::Duration::from_secs(10);
    let mut entries = vec![(50.0, 50.0, old_time)];

    std::thread::sleep(std::time::Duration::from_millis(5));

    let (_verts, _) = emit_cursor_magnetism(&ctx, &mut entries);

    // Old entry should be pruned, new one added (cursor jumped from 50,50 to 200,100)
    assert_eq!(entries.len(), 1);
    assert!(entries[0].2.elapsed().as_millis() < 100);
    // New entry should be near cursor position (205, 110 = cursor center)
    assert!((entries[0].0 - 205.0).abs() < 1.0);
    assert!((entries[0].1 - 110.0).abs() < 1.0);
}

// ========================================================================
// emit_line_number_pulse tests
// ========================================================================

#[test]
fn test_line_number_pulse_disabled() {
    let mut config = EffectsConfig::default();
    config.line_number_pulse.enabled = false;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    let (verts, needs_redraw) = emit_line_number_pulse(&ctx);
    assert_eq!(verts.len(), 0);
    assert_eq!(needs_redraw, false);
}

#[test]
fn test_line_number_pulse_with_cursor_and_window() {
    let mut config = EffectsConfig::default();
    config.line_number_pulse.enabled = true;
    config.line_number_pulse.intensity = 0.8;
    config.line_number_pulse.cycle_ms = 1000;

    let mut fgb = FrameGlyphBuffer::default();
    fgb.window_infos
        .push(make_selected_window_info(0.0, 0.0, 800.0, 600.0));

    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    let (verts, needs_redraw) = emit_line_number_pulse(&ctx);

    assert_eq!(needs_redraw, true);
    // Should produce at least one rect (6 vertices) for the gutter
    if verts.len() > 0 {
        validate_vertex_count(&verts);
        validate_vertices(&verts);
    }
}

// ========================================================================
// emit_cursor_spotlight tests
// ========================================================================

#[test]
fn test_cursor_spotlight_disabled() {
    let mut config = EffectsConfig::default();
    config.cursor_spotlight.enabled = false;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    let verts = emit_cursor_spotlight(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_cursor_spotlight_no_cursor() {
    let mut config = EffectsConfig::default();
    config.cursor_spotlight.enabled = true;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = None;

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    let verts = emit_cursor_spotlight(&ctx);
    assert_eq!(verts.len(), 0);
}

// ========================================================================
// emit_cursor_comet tests
// ========================================================================

#[test]
fn test_cursor_comet_disabled() {
    let mut config = EffectsConfig::default();
    config.cursor_comet.enabled = false;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);
    let mut positions = Vec::new();

    let (verts, needs_redraw) = emit_cursor_comet(&ctx, &mut positions);
    assert_eq!(verts.len(), 0);
    assert_eq!(needs_redraw, false);
}

// ========================================================================
// emit_cursor_particles tests
// ========================================================================

#[test]
fn test_cursor_particles_disabled() {
    let mut config = EffectsConfig::default();
    config.cursor_particles.enabled = false;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);
    let mut particles = Vec::new();
    let mut prev_pos = None;

    let (verts, needs_redraw) = emit_cursor_particles(&ctx, &mut particles, &mut prev_pos);
    assert_eq!(verts.len(), 0);
    assert_eq!(needs_redraw, false);
}

#[test]
fn test_cursor_particles_emits_on_movement() {
    let mut config = EffectsConfig::default();
    config.cursor_particles.enabled = true;
    config.cursor_particles.count = 5;
    config.cursor_particles.lifetime_ms = 1000;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);
    let mut particles = Vec::new();
    let mut prev_pos = Some((50.0, 50.0)); // Previous position far from current

    let (verts, _) = emit_cursor_particles(&ctx, &mut particles, &mut prev_pos);

    // Should have created particles
    assert!(
        particles.len() >= 5,
        "should emit at least 5 particles on movement"
    );
    if verts.len() > 0 {
        validate_vertex_count(&verts);
        validate_vertices(&verts);
    }
}

// ========================================================================
// emit_cursor_drop_shadow tests
// ========================================================================

#[test]
fn test_cursor_drop_shadow_disabled() {
    let mut config = EffectsConfig::default();
    config.cursor_shadow.enabled = false;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    let verts = emit_cursor_drop_shadow(&ctx);
    assert_eq!(verts.len(), 0);
}

// ========================================================================
// emit_cursor_trail_fade tests
// ========================================================================

#[test]
fn test_cursor_trail_fade_disabled() {
    let mut config = EffectsConfig::default();
    config.cursor_trail_fade.enabled = false;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);
    let mut positions = Vec::new();
    let fade_dur = std::time::Duration::from_millis(300);

    let (verts, needs_redraw) = emit_cursor_trail_fade(&ctx, &mut positions, &fade_dur);
    assert_eq!(verts.len(), 0);
    assert_eq!(needs_redraw, false);
}

#[test]
fn test_cursor_trail_fade_prunes_old_positions() {
    let mut config = EffectsConfig::default();
    config.cursor_trail_fade.enabled = true;
    config.cursor_trail_fade.ms = 100;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    // Add old position
    let old_time = crate::clock::Instant::now() - std::time::Duration::from_secs(10);
    let mut positions = vec![(50.0, 50.0, 10.0, 20.0, old_time)];
    let fade_dur = std::time::Duration::from_millis(100);

    let (verts, _) = emit_cursor_trail_fade(&ctx, &mut positions, &fade_dur);

    // Old position should be pruned
    assert_eq!(
        positions.len(),
        0,
        "expired trail positions should be pruned"
    );
    assert_eq!(verts.len(), 0);
}

// ========================================================================
// General property tests
// ========================================================================

#[test]
fn test_all_effects_produce_valid_vertices() {
    let mut config = EffectsConfig::default();

    // Enable a variety of cursor effects
    config.cursor_glow.enabled = true;
    config.cursor_crosshair.enabled = true;
    config.cursor_magnetism.enabled = true;

    let mut fgb = FrameGlyphBuffer::default();
    fgb.window_infos
        .push(make_selected_window_info(0.0, 0.0, 800.0, 600.0));

    let anim_cursor = Some(make_animated_cursor(400.0, 300.0, 10.0, 20.0, 1));

    let ctx = make_ctx(&config, &fgb, &anim_cursor, true);

    // Test cursor_glow
    let cursor_pulse_start = crate::clock::Instant::now();
    let verts = emit_cursor_glow(&ctx, &cursor_pulse_start);
    validate_vertex_count(&verts);
    validate_vertices(&verts);

    // Test cursor_crosshair
    let verts = emit_cursor_crosshair(&ctx);
    validate_vertex_count(&verts);
    validate_vertices(&verts);

    // Test cursor_magnetism
    let mut entries = Vec::new();
    let (verts, _) = emit_cursor_magnetism(&ctx, &mut entries);
    validate_vertex_count(&verts);
    validate_vertices(&verts);
}

#[test]
fn test_effects_respect_cursor_visible_flag() {
    let mut config = EffectsConfig::default();
    config.cursor_glow.enabled = true;
    config.cursor_crosshair.enabled = true;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));

    // Test with cursor_visible = false
    let ctx = make_ctx(&config, &fgb, &anim_cursor, false);

    let cursor_pulse_start = crate::clock::Instant::now();
    let verts = emit_cursor_glow(&ctx, &cursor_pulse_start);
    assert_eq!(verts.len(), 0, "glow should respect cursor_visible=false");

    let verts = emit_cursor_crosshair(&ctx);
    assert_eq!(
        verts.len(),
        0,
        "crosshair should respect cursor_visible=false"
    );
}

#[test]
fn test_cursor_glow_layer_count_calculation() {
    let mut config = EffectsConfig::default();
    config.cursor_glow.enabled = true;

    let fgb = FrameGlyphBuffer::default();
    let anim_cursor = Some(make_animated_cursor(100.0, 100.0, 10.0, 20.0, 1));
    let cursor_pulse_start = crate::clock::Instant::now();

    // Test different radius values
    for radius in [10.0_f32, 20.0, 30.0, 50.0, 100.0] {
        config.cursor_glow.radius = radius;
        let ctx = make_ctx(&config, &fgb, &anim_cursor, true);
        let verts = emit_cursor_glow(&ctx, &cursor_pulse_start);

        let expected_layers = (radius / 2.0).ceil() as usize;
        let expected_verts = expected_layers * 6;

        assert_eq!(
            verts.len(),
            expected_verts,
            "radius {} should produce {} layers ({} verts)",
            radius,
            expected_layers,
            expected_verts
        );
        validate_vertex_count(&verts);
    }
}
