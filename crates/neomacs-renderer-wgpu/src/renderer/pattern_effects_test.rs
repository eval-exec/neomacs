use super::*;
use crate::effect_config::EffectsConfig;
use neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer;

/// Helper to create an EffectCtx for testing
fn make_ctx<'a>(effects: &'a EffectsConfig, fgb: &'a FrameGlyphBuffer) -> EffectCtx<'a> {
    EffectCtx {
        effects,
        frame_glyphs: fgb,
        animated_cursor: &None,
        cursor_visible: false,
        mouse_pos: (0.0, 0.0),
        surface_width: 800,
        surface_height: 600,
        aurora_start: std::time::Instant::now(),
        scale_factor: 1.0,
        logical_w: 800.0,
        logical_h: 600.0,
        renderer_width: 800.0,
        renderer_height: 600.0,
    }
}

/// Helper to check that all vertices have valid positions and colors
fn validate_vertices(vertices: &[RectVertex]) {
    for v in vertices {
        assert!(v.position[0].is_finite(), "x position is not finite");
        assert!(v.position[1].is_finite(), "y position is not finite");
        assert!(v.color[0].is_finite() && v.color[0] >= 0.0, "r is invalid");
        assert!(v.color[1].is_finite() && v.color[1] >= 0.0, "g is invalid");
        assert!(v.color[2].is_finite() && v.color[2] >= 0.0, "b is invalid");
        assert!(v.color[3].is_finite() && v.color[3] >= 0.0, "a is invalid");
    }
}

// ========================================================================
// emit_background_pattern tests
// ========================================================================

#[test]
fn test_background_pattern_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_background_pattern(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_background_pattern_dots() {
    let mut effects = EffectsConfig::default();
    effects.bg_pattern.style = 1; // dots
    effects.bg_pattern.spacing = 20.0;
    let mut fgb = FrameGlyphBuffer::new();
    fgb.width = 800.0;
    fgb.height = 600.0;
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_background_pattern(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

#[test]
fn test_background_pattern_grid() {
    let mut effects = EffectsConfig::default();
    effects.bg_pattern.style = 2; // grid
    effects.bg_pattern.spacing = 30.0;
    let mut fgb = FrameGlyphBuffer::new();
    fgb.width = 800.0;
    fgb.height = 600.0;
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_background_pattern(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

#[test]
fn test_background_pattern_crosshatch() {
    let mut effects = EffectsConfig::default();
    effects.bg_pattern.style = 3; // crosshatch
    effects.bg_pattern.spacing = 25.0;
    let mut fgb = FrameGlyphBuffer::new();
    fgb.width = 800.0;
    fgb.height = 600.0;
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_background_pattern(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_heat_distortion tests
// ========================================================================

#[test]
fn test_heat_distortion_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_heat_distortion(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_heat_distortion_enabled() {
    let mut effects = EffectsConfig::default();
    effects.heat_distortion.enabled = true;
    effects.heat_distortion.intensity = 0.5;
    effects.heat_distortion.edge_width = 30.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_heat_distortion(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_neon_border tests
// ========================================================================

#[test]
fn test_neon_border_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_neon_border(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_neon_border_enabled() {
    let mut effects = EffectsConfig::default();
    effects.neon_border.enabled = true;
    effects.neon_border.thickness = 3.0;
    effects.neon_border.intensity = 0.6;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_neon_border(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_plasma_border tests
// ========================================================================

#[test]
fn test_plasma_border_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_plasma_border(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_plasma_border_enabled() {
    let mut effects = EffectsConfig::default();
    effects.plasma_border.enabled = true;
    effects.plasma_border.width = 4.0;
    effects.plasma_border.speed = 1.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_plasma_border(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_topographic_contour tests
// ========================================================================

#[test]
fn test_topographic_contour_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_topographic_contour(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_topographic_contour_enabled() {
    let mut effects = EffectsConfig::default();
    effects.topo_contour.enabled = true;
    effects.topo_contour.spacing = 30.0;
    effects.topo_contour.speed = 1.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_topographic_contour(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_constellation tests
// ========================================================================

#[test]
fn test_constellation_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_constellation(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_constellation_enabled() {
    let mut effects = EffectsConfig::default();
    effects.constellation.enabled = true;
    effects.constellation.star_count = 20;
    effects.constellation.connect_dist = 80.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_constellation(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_kaleidoscope tests
// ========================================================================

#[test]
fn test_kaleidoscope_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_kaleidoscope(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_kaleidoscope_enabled() {
    let mut effects = EffectsConfig::default();
    effects.kaleidoscope.enabled = true;
    effects.kaleidoscope.segments = 6;
    effects.kaleidoscope.speed = 0.5;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_kaleidoscope(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_noise_field tests
// ========================================================================

#[test]
fn test_noise_field_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_noise_field(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_noise_field_enabled() {
    let mut effects = EffectsConfig::default();
    effects.noise_field.enabled = true;
    effects.noise_field.scale = 50.0;
    effects.noise_field.speed = 0.5;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_noise_field(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_spiral_vortex tests
// ========================================================================

#[test]
fn test_spiral_vortex_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_spiral_vortex(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_spiral_vortex_enabled() {
    let mut effects = EffectsConfig::default();
    effects.spiral_vortex.enabled = true;
    effects.spiral_vortex.arms = 4;
    effects.spiral_vortex.speed = 0.5;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_spiral_vortex(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_diamond_lattice tests
// ========================================================================

#[test]
fn test_diamond_lattice_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_diamond_lattice(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_diamond_lattice_enabled() {
    let mut effects = EffectsConfig::default();
    effects.diamond_lattice.enabled = true;
    effects.diamond_lattice.cell_size = 30.0;
    effects.diamond_lattice.shimmer_speed = 0.8;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_diamond_lattice(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_wave_interference tests
// ========================================================================

#[test]
fn test_wave_interference_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_wave_interference(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_wave_interference_enabled() {
    let mut effects = EffectsConfig::default();
    effects.wave_interference.enabled = true;
    effects.wave_interference.wavelength = 30.0;
    effects.wave_interference.source_count = 3;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_wave_interference(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_chevron tests
// ========================================================================

#[test]
fn test_chevron_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_chevron(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_chevron_enabled() {
    let mut effects = EffectsConfig::default();
    effects.chevron_pattern.enabled = true;
    effects.chevron_pattern.spacing = 40.0;
    effects.chevron_pattern.speed = 0.5;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_chevron(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_sunburst tests
// ========================================================================

#[test]
fn test_sunburst_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_sunburst(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_sunburst_enabled() {
    let mut effects = EffectsConfig::default();
    effects.sunburst_pattern.enabled = true;
    effects.sunburst_pattern.ray_count = 12;
    effects.sunburst_pattern.speed = 0.5;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_sunburst(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_honeycomb_dissolve tests
// ========================================================================

#[test]
fn test_honeycomb_dissolve_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_honeycomb_dissolve(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_honeycomb_dissolve_enabled() {
    let mut effects = EffectsConfig::default();
    effects.honeycomb_dissolve.enabled = true;
    effects.honeycomb_dissolve.cell_size = 30.0;
    effects.honeycomb_dissolve.speed = 0.8;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_honeycomb_dissolve(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_moire tests
// ========================================================================

#[test]
fn test_moire_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_moire(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_moire_enabled() {
    let mut effects = EffectsConfig::default();
    effects.moire_pattern.enabled = true;
    effects.moire_pattern.line_spacing = 8.0;
    effects.moire_pattern.angle_offset = 5.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_moire(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_dot_matrix tests
// ========================================================================

#[test]
fn test_dot_matrix_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_dot_matrix(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_dot_matrix_enabled() {
    let mut effects = EffectsConfig::default();
    effects.dot_matrix.enabled = true;
    effects.dot_matrix.spacing = 12.0;
    effects.dot_matrix.pulse_speed = 1.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_dot_matrix(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_concentric_rings tests
// ========================================================================

#[test]
fn test_concentric_rings_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_concentric_rings(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_concentric_rings_enabled() {
    let mut effects = EffectsConfig::default();
    effects.concentric_rings.enabled = true;
    effects.concentric_rings.spacing = 30.0;
    effects.concentric_rings.expansion_speed = 1.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_concentric_rings(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_zigzag tests
// ========================================================================

#[test]
fn test_zigzag_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_zigzag(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_zigzag_enabled() {
    let mut effects = EffectsConfig::default();
    effects.zigzag_pattern.enabled = true;
    effects.zigzag_pattern.amplitude = 15.0;
    effects.zigzag_pattern.frequency = 0.1;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_zigzag(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_tessellation tests
// ========================================================================

#[test]
fn test_tessellation_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_tessellation(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_tessellation_enabled() {
    let mut effects = EffectsConfig::default();
    effects.tessellation.enabled = true;
    effects.tessellation.tile_size = 40.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_tessellation(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_guilloche tests
// ========================================================================

#[test]
fn test_guilloche_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_guilloche(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_guilloche_enabled() {
    let mut effects = EffectsConfig::default();
    effects.guilloche.enabled = true;
    effects.guilloche.curve_count = 8;
    effects.guilloche.wave_freq = 1.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_guilloche(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_celtic_knot tests
// ========================================================================

#[test]
fn test_celtic_knot_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_celtic_knot(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_celtic_knot_enabled() {
    let mut effects = EffectsConfig::default();
    effects.celtic_knot.enabled = true;
    effects.celtic_knot.scale = 60.0;
    effects.celtic_knot.weave_speed = 1.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_celtic_knot(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_argyle tests
// ========================================================================

#[test]
fn test_argyle_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_argyle(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_argyle_enabled() {
    let mut effects = EffectsConfig::default();
    effects.argyle_pattern.enabled = true;
    effects.argyle_pattern.diamond_size = 30.0;
    effects.argyle_pattern.line_width = 1.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_argyle(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_basket_weave tests
// ========================================================================

#[test]
fn test_basket_weave_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_basket_weave(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_basket_weave_enabled() {
    let mut effects = EffectsConfig::default();
    effects.basket_weave.enabled = true;
    effects.basket_weave.strip_width = 6.0;
    effects.basket_weave.strip_spacing = 20.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_basket_weave(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_fish_scale tests
// ========================================================================

#[test]
fn test_fish_scale_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_fish_scale(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_fish_scale_enabled() {
    let mut effects = EffectsConfig::default();
    effects.fish_scale.enabled = true;
    effects.fish_scale.size = 16.0;
    effects.fish_scale.row_offset = 0.5;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_fish_scale(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_trefoil_knot tests
// ========================================================================

#[test]
fn test_trefoil_knot_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_trefoil_knot(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_trefoil_knot_enabled() {
    let mut effects = EffectsConfig::default();
    effects.trefoil_knot.enabled = true;
    effects.trefoil_knot.size = 80.0;
    effects.trefoil_knot.rotation_speed = 1.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_trefoil_knot(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_herringbone tests
// ========================================================================

#[test]
fn test_herringbone_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_herringbone(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_herringbone_enabled() {
    let mut effects = EffectsConfig::default();
    effects.herringbone_pattern.enabled = true;
    effects.herringbone_pattern.tile_width = 20.0;
    effects.herringbone_pattern.tile_height = 10.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_herringbone(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_target_reticle tests
// ========================================================================

#[test]
fn test_target_reticle_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_target_reticle(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_target_reticle_enabled() {
    let mut effects = EffectsConfig::default();
    effects.target_reticle.enabled = true;
    effects.target_reticle.ring_count = 3;
    effects.target_reticle.pulse_speed = 1.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_target_reticle(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_plaid tests
// ========================================================================

#[test]
fn test_plaid_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_plaid(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_plaid_enabled() {
    let mut effects = EffectsConfig::default();
    effects.plaid_pattern.enabled = true;
    effects.plaid_pattern.band_width = 4.0;
    effects.plaid_pattern.band_spacing = 30.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_plaid(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_brick_wall tests
// ========================================================================

#[test]
fn test_brick_wall_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_brick_wall(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_brick_wall_enabled() {
    let mut effects = EffectsConfig::default();
    effects.brick_wall.enabled = true;
    effects.brick_wall.width = 40.0;
    effects.brick_wall.height = 20.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_brick_wall(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_sine_wave tests
// ========================================================================

#[test]
fn test_sine_wave_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_sine_wave(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_sine_wave_enabled() {
    let mut effects = EffectsConfig::default();
    effects.sine_wave.enabled = true;
    effects.sine_wave.amplitude = 20.0;
    effects.sine_wave.wavelength = 80.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_sine_wave(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_rotating_gear tests
// ========================================================================

#[test]
fn test_rotating_gear_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_rotating_gear(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_rotating_gear_enabled() {
    let mut effects = EffectsConfig::default();
    effects.rotating_gear.enabled = true;
    effects.rotating_gear.size = 40.0;
    effects.rotating_gear.speed = 0.5;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_rotating_gear(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_crosshatch tests
// ========================================================================

#[test]
fn test_crosshatch_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_crosshatch(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_crosshatch_enabled() {
    let mut effects = EffectsConfig::default();
    effects.crosshatch_pattern.enabled = true;
    effects.crosshatch_pattern.line_spacing = 20.0;
    effects.crosshatch_pattern.angle = 45.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_crosshatch(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_hex_grid tests
// ========================================================================

#[test]
fn test_hex_grid_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_hex_grid(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_hex_grid_enabled() {
    let mut effects = EffectsConfig::default();
    effects.hex_grid.enabled = true;
    effects.hex_grid.cell_size = 40.0;
    effects.hex_grid.pulse_speed = 1.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_hex_grid(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_circuit_board tests
// ========================================================================

#[test]
fn test_circuit_board_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_circuit_board(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_circuit_board_enabled() {
    let mut effects = EffectsConfig::default();
    effects.circuit_trace.enabled = true;
    effects.circuit_trace.width = 2.0;
    effects.circuit_trace.speed = 1.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_circuit_board(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_warp_grid tests
// ========================================================================

#[test]
fn test_warp_grid_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_warp_grid(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_warp_grid_enabled() {
    let mut effects = EffectsConfig::default();
    effects.warp_grid.enabled = true;
    effects.warp_grid.density = 20;
    effects.warp_grid.amplitude = 5.0;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_warp_grid(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}

// ========================================================================
// emit_prism_rainbow_edge tests
// ========================================================================

#[test]
fn test_prism_rainbow_edge_disabled() {
    let effects = EffectsConfig::default();
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_prism_rainbow_edge(&ctx);
    assert_eq!(verts.len(), 0);
}

#[test]
fn test_prism_rainbow_edge_enabled() {
    let mut effects = EffectsConfig::default();
    effects.prism_edge.enabled = true;
    effects.prism_edge.width = 6.0;
    effects.prism_edge.saturation = 0.8;
    let fgb = FrameGlyphBuffer::new();
    let ctx = make_ctx(&effects, &fgb);

    let verts = emit_prism_rainbow_edge(&ctx);
    assert!(verts.len() > 0);
    assert_eq!(verts.len() % 6, 0);
    validate_vertices(&verts);
}
