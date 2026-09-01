use super::*;

#[test]
fn test_scroll_effect_from_str() {
    assert_eq!(TransitionEffect::from_str("slide"), TransitionEffect::Slide);
    assert_eq!(
        TransitionEffect::from_str("wobbly"),
        TransitionEffect::Wobbly
    );
    assert_eq!(
        TransitionEffect::from_str("jelly"),
        TransitionEffect::Wobbly
    );
    assert_eq!(
        TransitionEffect::from_str("page-curl"),
        TransitionEffect::PageCurl
    );
    assert_eq!(
        TransitionEffect::from_str("chromatic-aberration"),
        TransitionEffect::ChromaticAberration
    );
    assert_eq!(
        TransitionEffect::from_str("unknown"),
        TransitionEffect::Slide
    );
}

#[test]
fn test_scroll_effect_roundtrip() {
    for effect in TransitionEffect::ALL.iter() {
        assert_eq!(TransitionEffect::from_str(effect.as_str()), *effect);
    }
}

#[test]
fn test_scroll_easing_apply() {
    // EaseOutQuad: starts fast, ends slow
    assert!(TransitionEasing::EaseOutQuad.apply(0.5) > 0.5);
    assert_eq!(TransitionEasing::EaseOutQuad.apply(0.0), 0.0);
    assert_eq!(TransitionEasing::EaseOutQuad.apply(1.0), 1.0);

    // Linear
    let v = TransitionEasing::Linear.apply(0.5);
    assert!((v - 0.5).abs() < 0.001);

    // Spring: should converge to 1.0
    assert!(TransitionEasing::Spring.apply(0.8) > 0.95);
}

#[test]
fn test_spring_state() {
    let mut spring = SpringState::new(12.0);
    let mut settled = false;
    for _ in 0..200 {
        settled = spring.step(1.0 / 60.0);
        if settled {
            break;
        }
    }
    assert!(settled);
    assert!((spring.position - 1.0).abs() < 0.01);
}

#[test]
fn test_per_line_spring() {
    let mut state = PerLineSpringState::new(10, 12.0, 0.01);
    for _ in 0..300 {
        if state.step(1.0 / 60.0) {
            break;
        }
    }
    // All lines should have reached target
    for i in 0..10 {
        assert!(
            (state.line_offset(i) - 1.0).abs() < 0.05,
            "Line {} offset: {}",
            i,
            state.line_offset(i)
        );
    }
}

#[test]
fn test_tessellate_quad() {
    let verts = tessellate_quad_strips(
        0.0,
        0.0,
        100.0,
        200.0,
        0.0,
        0.0,
        1.0,
        1.0,
        4, // 4 strips
        0.0,
        |_, _, _| (0.0, 0.0), // no deformation
    );
    assert_eq!(verts.len(), 4 * 6); // 4 strips × 6 vertices
}

#[test]
fn test_noise2d_deterministic() {
    let a = noise2d(1.0, 2.0);
    let b = noise2d(1.0, 2.0);
    assert_eq!(a, b);
    // Different inputs should give different results (usually)
    let c = noise2d(3.0, 4.0);
    assert_ne!(a, c);
}

#[test]
fn test_post_process_params() {
    let p = PostProcessParams {
        scroll_velocity: 500.0,
        scroll_speed: 0.5,
        scroll_direction: 1.0,
        scroll_position: 100.0,
        time: 1.0,
    };
    assert!(p.motion_blur_offset() > 0.0);
    assert!(p.chromatic_offset() > 0.0);
    assert!(p.ghost_opacity() > 0.0);
    assert!(p.color_temp_shift() > 0.0);
}

// ── TransitionEffect additional tests ────────────────────────────────────

#[test]
fn test_scroll_effect_count_matches_all() {
    assert_eq!(TransitionEffect::ALL.len(), TransitionEffect::COUNT);
}

#[test]
fn test_scroll_effect_default_is_slide() {
    assert_eq!(TransitionEffect::default(), TransitionEffect::Slide);
}

#[test]
fn test_scroll_effect_from_str_case_insensitive() {
    assert_eq!(TransitionEffect::from_str("SLIDE"), TransitionEffect::Slide);
    assert_eq!(
        TransitionEffect::from_str("Crossfade"),
        TransitionEffect::Crossfade
    );
    assert_eq!(
        TransitionEffect::from_str("WOBBLY"),
        TransitionEffect::Wobbly
    );
    assert_eq!(
        TransitionEffect::from_str("Motion-Blur"),
        TransitionEffect::MotionBlur
    );
    assert_eq!(
        TransitionEffect::from_str("CRT-Scanlines"),
        TransitionEffect::CRTScanlines
    );
}

#[test]
fn test_scroll_effect_from_str_underscore_variants() {
    // Underscores are converted to hyphens before matching
    assert_eq!(
        TransitionEffect::from_str("scale_zoom"),
        TransitionEffect::ScaleZoom
    );
    assert_eq!(
        TransitionEffect::from_str("page_curl"),
        TransitionEffect::PageCurl
    );
    assert_eq!(
        TransitionEffect::from_str("per_line_spring"),
        TransitionEffect::PerLineSpring
    );
    assert_eq!(
        TransitionEffect::from_str("ghost_trails"),
        TransitionEffect::GhostTrails
    );
    assert_eq!(
        TransitionEffect::from_str("color_temperature"),
        TransitionEffect::ColorTemperature
    );
}

#[test]
fn test_scroll_effect_from_str_all_aliases() {
    // ScaleZoom aliases
    assert_eq!(
        TransitionEffect::from_str("scalezoom"),
        TransitionEffect::ScaleZoom
    );
    assert_eq!(
        TransitionEffect::from_str("zoom"),
        TransitionEffect::ScaleZoom
    );
    // FadeEdges aliases
    assert_eq!(
        TransitionEffect::from_str("fadeedges"),
        TransitionEffect::FadeEdges
    );
    assert_eq!(
        TransitionEffect::from_str("fade"),
        TransitionEffect::FadeEdges
    );
    // Cascade aliases
    assert_eq!(
        TransitionEffect::from_str("waterfall"),
        TransitionEffect::Cascade
    );
    // Parallax aliases
    assert_eq!(
        TransitionEffect::from_str("depth"),
        TransitionEffect::Parallax
    );
    // Tilt aliases
    assert_eq!(
        TransitionEffect::from_str("perspective"),
        TransitionEffect::Tilt
    );
    // PageCurl aliases
    assert_eq!(
        TransitionEffect::from_str("curl"),
        TransitionEffect::PageCurl
    );
    // CardFlip aliases
    assert_eq!(
        TransitionEffect::from_str("cardflip"),
        TransitionEffect::CardFlip
    );
    assert_eq!(
        TransitionEffect::from_str("flip"),
        TransitionEffect::CardFlip
    );
    // CylinderRoll aliases
    assert_eq!(
        TransitionEffect::from_str("cylinderroll"),
        TransitionEffect::CylinderRoll
    );
    assert_eq!(
        TransitionEffect::from_str("cylinder"),
        TransitionEffect::CylinderRoll
    );
    assert_eq!(
        TransitionEffect::from_str("roll"),
        TransitionEffect::CylinderRoll
    );
    // Wobbly aliases
    assert_eq!(
        TransitionEffect::from_str("wobble"),
        TransitionEffect::Wobbly
    );
    // Wave aliases
    assert_eq!(TransitionEffect::from_str("sine"), TransitionEffect::Wave);
    // PerLineSpring aliases
    assert_eq!(
        TransitionEffect::from_str("perlinespring"),
        TransitionEffect::PerLineSpring
    );
    assert_eq!(
        TransitionEffect::from_str("line-spring"),
        TransitionEffect::PerLineSpring
    );
    assert_eq!(
        TransitionEffect::from_str("slinky"),
        TransitionEffect::PerLineSpring
    );
    // Liquid aliases
    assert_eq!(
        TransitionEffect::from_str("fluid"),
        TransitionEffect::Liquid
    );
    assert_eq!(
        TransitionEffect::from_str("water"),
        TransitionEffect::Liquid
    );
    // MotionBlur aliases
    assert_eq!(
        TransitionEffect::from_str("motionblur"),
        TransitionEffect::MotionBlur
    );
    assert_eq!(
        TransitionEffect::from_str("blur"),
        TransitionEffect::MotionBlur
    );
    // ChromaticAberration aliases
    assert_eq!(
        TransitionEffect::from_str("chromaticaberration"),
        TransitionEffect::ChromaticAberration
    );
    assert_eq!(
        TransitionEffect::from_str("chromatic"),
        TransitionEffect::ChromaticAberration
    );
    assert_eq!(
        TransitionEffect::from_str("aberration"),
        TransitionEffect::ChromaticAberration
    );
    // GhostTrails aliases
    assert_eq!(
        TransitionEffect::from_str("ghosttrails"),
        TransitionEffect::GhostTrails
    );
    assert_eq!(
        TransitionEffect::from_str("ghost"),
        TransitionEffect::GhostTrails
    );
    assert_eq!(
        TransitionEffect::from_str("trails"),
        TransitionEffect::GhostTrails
    );
    // ColorTemperature aliases
    assert_eq!(
        TransitionEffect::from_str("colortemperature"),
        TransitionEffect::ColorTemperature
    );
    assert_eq!(
        TransitionEffect::from_str("color-temp"),
        TransitionEffect::ColorTemperature
    );
    assert_eq!(
        TransitionEffect::from_str("temperature"),
        TransitionEffect::ColorTemperature
    );
    // CRTScanlines aliases
    assert_eq!(
        TransitionEffect::from_str("crtscanlines"),
        TransitionEffect::CRTScanlines
    );
    assert_eq!(
        TransitionEffect::from_str("crt"),
        TransitionEffect::CRTScanlines
    );
    assert_eq!(
        TransitionEffect::from_str("scanlines"),
        TransitionEffect::CRTScanlines
    );
    // DepthOfField aliases
    assert_eq!(
        TransitionEffect::from_str("depthoffield"),
        TransitionEffect::DepthOfField
    );
    assert_eq!(
        TransitionEffect::from_str("dof"),
        TransitionEffect::DepthOfField
    );
    // TypewriterReveal aliases
    assert_eq!(
        TransitionEffect::from_str("typewriterreveal"),
        TransitionEffect::TypewriterReveal
    );
    assert_eq!(
        TransitionEffect::from_str("typewriter"),
        TransitionEffect::TypewriterReveal
    );
}

#[test]
fn test_scroll_effect_needs_post_process() {
    let pp_effects = [
        TransitionEffect::MotionBlur,
        TransitionEffect::ChromaticAberration,
        TransitionEffect::GhostTrails,
        TransitionEffect::ColorTemperature,
        TransitionEffect::CRTScanlines,
        TransitionEffect::DepthOfField,
    ];
    for effect in &pp_effects {
        assert!(
            effect.needs_post_process(),
            "{:?} should need post-processing",
            effect
        );
    }
}

#[test]
fn test_scroll_effect_non_post_process() {
    let non_pp = [
        TransitionEffect::Slide,
        TransitionEffect::Crossfade,
        TransitionEffect::ScaleZoom,
        TransitionEffect::FadeEdges,
        TransitionEffect::Cascade,
        TransitionEffect::Parallax,
        TransitionEffect::Tilt,
        TransitionEffect::PageCurl,
        TransitionEffect::CardFlip,
        TransitionEffect::CylinderRoll,
        TransitionEffect::Wobbly,
        TransitionEffect::Wave,
        TransitionEffect::PerLineSpring,
        TransitionEffect::Liquid,
        TransitionEffect::TypewriterReveal,
    ];
    for effect in &non_pp {
        assert!(
            !effect.needs_post_process(),
            "{:?} should NOT need post-processing",
            effect
        );
    }
}

#[test]
fn test_scroll_effect_needs_tessellation() {
    let tess_effects = [
        TransitionEffect::Wobbly,
        TransitionEffect::Wave,
        TransitionEffect::PerLineSpring,
        TransitionEffect::Liquid,
        TransitionEffect::Cascade,
        TransitionEffect::CylinderRoll,
        TransitionEffect::PageCurl,
        TransitionEffect::TypewriterReveal,
    ];
    for effect in &tess_effects {
        assert!(
            effect.needs_tessellation(),
            "{:?} should need tessellation",
            effect
        );
    }
    // A few that should NOT need tessellation
    assert!(!TransitionEffect::Slide.needs_tessellation());
    assert!(!TransitionEffect::Crossfade.needs_tessellation());
    assert!(!TransitionEffect::MotionBlur.needs_tessellation());
}

#[test]
fn test_scroll_effect_needs_3d() {
    let three_d = [
        TransitionEffect::Tilt,
        TransitionEffect::PageCurl,
        TransitionEffect::CardFlip,
        TransitionEffect::CylinderRoll,
    ];
    for effect in &three_d {
        assert!(effect.needs_3d(), "{:?} should need 3D", effect);
    }
    // A few that should NOT need 3D
    assert!(!TransitionEffect::Slide.needs_3d());
    assert!(!TransitionEffect::Wobbly.needs_3d());
    assert!(!TransitionEffect::MotionBlur.needs_3d());
    assert!(!TransitionEffect::Crossfade.needs_3d());
}

// ── TransitionEasing additional tests ────────────────────────────────────

#[test]
fn test_scroll_easing_default_is_ease_out_quad() {
    assert_eq!(TransitionEasing::default(), TransitionEasing::EaseOutQuad);
}

#[test]
fn test_scroll_easing_roundtrip() {
    let easings = [
        TransitionEasing::EaseOutQuad,
        TransitionEasing::EaseOutCubic,
        TransitionEasing::Spring,
        TransitionEasing::Linear,
        TransitionEasing::EaseInOutCubic,
    ];
    for easing in &easings {
        assert_eq!(
            TransitionEasing::from_str(easing.as_str()),
            *easing,
            "Roundtrip failed for {:?}",
            easing
        );
    }
}

#[test]
fn test_scroll_easing_clamps_input() {
    // Negative values should clamp to 0
    assert_eq!(TransitionEasing::Linear.apply(-1.0), 0.0);
    assert_eq!(TransitionEasing::EaseOutQuad.apply(-0.5), 0.0);
    // Values > 1 should clamp to 1
    assert_eq!(TransitionEasing::Linear.apply(2.0), 1.0);
    assert_eq!(TransitionEasing::EaseOutCubic.apply(1.5), 1.0);
    // Spring at clamped t=1 is very close to 1.0 but uses exponential decay
    let spring_at_max = TransitionEasing::Spring.apply(10.0);
    assert!(
        (spring_at_max - 1.0).abs() < 0.01,
        "Spring at clamped max should be ~1.0, got {}",
        spring_at_max
    );
}

#[test]
fn test_scroll_easing_all_boundaries() {
    let easings = [
        TransitionEasing::EaseOutQuad,
        TransitionEasing::EaseOutCubic,
        TransitionEasing::Spring,
        TransitionEasing::Linear,
        TransitionEasing::EaseInOutCubic,
    ];
    for easing in &easings {
        let at_zero = easing.apply(0.0);
        let at_one = easing.apply(1.0);
        assert!(
            at_zero.abs() < 0.001,
            "{:?} at t=0 should be ~0, got {}",
            easing,
            at_zero
        );
        // Spring uses exponential decay: 1-(1+w)*e^(-w) which doesn't
        // reach exactly 1.0 at t=1.0 for finite omega. Use wider tolerance.
        let tolerance = if *easing == TransitionEasing::Spring {
            0.01
        } else {
            0.001
        };
        assert!(
            (at_one - 1.0).abs() < tolerance,
            "{:?} at t=1 should be ~1, got {}",
            easing,
            at_one
        );
    }
}

#[test]
fn test_scroll_easing_monotonicity() {
    let easings = [
        TransitionEasing::EaseOutQuad,
        TransitionEasing::EaseOutCubic,
        TransitionEasing::Spring,
        TransitionEasing::Linear,
        TransitionEasing::EaseInOutCubic,
    ];
    for easing in &easings {
        let mut prev = easing.apply(0.0);
        for i in 1..=100 {
            let t = i as f32 / 100.0;
            let val = easing.apply(t);
            assert!(
                val >= prev - 0.001,
                "{:?} not monotonic at t={}: {} < {}",
                easing,
                t,
                val,
                prev
            );
            prev = val;
        }
    }
}

#[test]
fn test_scroll_easing_ease_out_cubic_deceleration() {
    // Ease-out cubic should produce > 0.5 at t=0.5 (front-loaded)
    let mid = TransitionEasing::EaseOutCubic.apply(0.5);
    assert!(
        mid > 0.5,
        "EaseOutCubic at 0.5 should be > 0.5, got {}",
        mid
    );
    // And it should be larger than EaseOutQuad at the same point
    let quad_mid = TransitionEasing::EaseOutQuad.apply(0.5);
    assert!(
        mid > quad_mid,
        "EaseOutCubic({}) should > EaseOutQuad({}) at t=0.5",
        mid,
        quad_mid
    );
}

#[test]
fn test_scroll_easing_ease_in_out_cubic_symmetry() {
    // EaseInOutCubic should be symmetric: f(0.5-x) + f(0.5+x) ≈ 1.0
    for i in 0..=10 {
        let x = i as f32 / 20.0; // 0.0, 0.05, ..., 0.5
        let left = TransitionEasing::EaseInOutCubic.apply(0.5 - x);
        let right = TransitionEasing::EaseInOutCubic.apply(0.5 + x);
        assert!(
            (left + right - 1.0).abs() < 0.01,
            "Symmetry broken at offset {}: f({})={}, f({})={}, sum={}",
            x,
            0.5 - x,
            left,
            0.5 + x,
            right,
            left + right
        );
    }
}

#[test]
fn test_scroll_easing_from_str_all_aliases() {
    assert_eq!(
        TransitionEasing::from_str("ease-out"),
        TransitionEasing::EaseOutQuad
    );
    assert_eq!(
        TransitionEasing::from_str("ease-out-quad"),
        TransitionEasing::EaseOutQuad
    );
    assert_eq!(
        TransitionEasing::from_str("quad"),
        TransitionEasing::EaseOutQuad
    );
    assert_eq!(
        TransitionEasing::from_str("ease-out-cubic"),
        TransitionEasing::EaseOutCubic
    );
    assert_eq!(
        TransitionEasing::from_str("cubic"),
        TransitionEasing::EaseOutCubic
    );
    assert_eq!(
        TransitionEasing::from_str("spring"),
        TransitionEasing::Spring
    );
    assert_eq!(
        TransitionEasing::from_str("damped"),
        TransitionEasing::Spring
    );
    assert_eq!(
        TransitionEasing::from_str("linear"),
        TransitionEasing::Linear
    );
    assert_eq!(
        TransitionEasing::from_str("ease-in-out"),
        TransitionEasing::EaseInOutCubic
    );
    assert_eq!(
        TransitionEasing::from_str("ease-in-out-cubic"),
        TransitionEasing::EaseInOutCubic
    );
    // Unknown falls back to EaseOutQuad
    assert_eq!(
        TransitionEasing::from_str("unknown"),
        TransitionEasing::EaseOutQuad
    );
    assert_eq!(
        TransitionEasing::from_str(""),
        TransitionEasing::EaseOutQuad
    );
}

// ── SpringState additional tests ─────────────────────────────────────

#[test]
fn test_spring_state_initial_values() {
    let spring = SpringState::new(10.0);
    assert_eq!(spring.position, 0.0);
    assert_eq!(spring.velocity, 0.0);
    assert_eq!(spring.target, 1.0);
    assert_eq!(spring.omega, 10.0);
}

#[test]
fn test_spring_state_high_omega_settles_faster() {
    let mut fast = SpringState::new(20.0);
    let mut slow = SpringState::new(5.0);
    let dt = 1.0 / 60.0;

    let mut fast_steps = 0;
    for i in 0..1000 {
        if fast.step(dt) {
            fast_steps = i;
            break;
        }
    }

    let mut slow_steps = 0;
    for i in 0..1000 {
        if slow.step(dt) {
            slow_steps = i;
            break;
        }
    }

    assert!(
        fast_steps < slow_steps,
        "High omega ({} steps) should settle before low omega ({} steps)",
        fast_steps,
        slow_steps
    );
}

#[test]
fn test_spring_state_zero_dt_no_change() {
    let mut spring = SpringState::new(12.0);
    let pos_before = spring.position;
    let vel_before = spring.velocity;
    spring.step(0.0);
    // With dt=0, exp(-w*0)=1, c1=x, c2=v+w*x
    // position = target + (c1 + c2*0)*1 = target + c1 = target + (pos-target) = pos
    assert_eq!(spring.position, pos_before);
    assert_eq!(spring.velocity, vel_before);
}

#[test]
fn test_spring_state_position_approaches_target_monotonically() {
    // For a critically damped spring starting at 0 with target 1,
    // position should monotonically increase toward target without overshoot
    let mut spring = SpringState::new(12.0);
    let dt = 1.0 / 60.0;
    let mut prev_pos = spring.position;
    for _ in 0..200 {
        if spring.step(dt) {
            break;
        }
        assert!(
            spring.position >= prev_pos - 0.001,
            "Spring position decreased from {} to {}",
            prev_pos,
            spring.position
        );
        assert!(
            spring.position <= spring.target + 0.01,
            "Spring overshot target: position {} > target {}",
            spring.position,
            spring.target
        );
        prev_pos = spring.position;
    }
}

// ── PerLineSpringState additional tests ──────────────────────────────

#[test]
fn test_per_line_spring_stagger_order() {
    let mut state = PerLineSpringState::new(5, 12.0, 0.05);
    let dt = 1.0 / 60.0;
    // Step a few frames so stagger is visible
    for _ in 0..10 {
        state.step(dt);
    }
    // Earlier lines should be further along than later lines
    for i in 0..4 {
        assert!(
            state.line_offset(i) >= state.line_offset(i + 1) - 0.001,
            "Line {} ({}) should be >= line {} ({})",
            i,
            state.line_offset(i),
            i + 1,
            state.line_offset(i + 1)
        );
    }
}

#[test]
fn test_per_line_spring_out_of_bounds_index() {
    let state = PerLineSpringState::new(3, 12.0, 0.01);
    // Out-of-bounds should return 0.0
    assert_eq!(state.line_offset(3), 0.0);
    assert_eq!(state.line_offset(100), 0.0);
    assert_eq!(state.line_offset(usize::MAX), 0.0);
}

#[test]
fn test_per_line_spring_zero_lines() {
    let mut state = PerLineSpringState::new(0, 12.0, 0.01);
    // With zero lines, should immediately settle
    assert!(state.step(1.0 / 60.0));
    assert_eq!(state.line_offset(0), 0.0);
}

// ── Tessellation / geometry additional tests ─────────────────────────

#[test]
fn test_tessellate_quad_with_deformation() {
    let no_deform = tessellate_quad_strips(
        0.0,
        0.0,
        100.0,
        200.0,
        0.0,
        0.0,
        1.0,
        1.0,
        2,
        0.0,
        |_, _, _| (0.0, 0.0),
    );
    let with_deform = tessellate_quad_strips(
        0.0,
        0.0,
        100.0,
        200.0,
        0.0,
        0.0,
        1.0,
        1.0,
        2,
        0.0,
        |_, _, _| (10.0, 5.0),
    );
    // Same number of vertices
    assert_eq!(no_deform.len(), with_deform.len());
    // But positions should differ
    assert_ne!(no_deform[0][0], with_deform[0][0]); // x differs
}

#[test]
fn test_make_quad_vertices_basic() {
    let verts = make_quad_vertices(0.0, 0.0, 100.0, 50.0, 0.0, 0.0, 1.0, 1.0, 1.0);
    assert_eq!(verts.len(), 6); // Two triangles
    // All vertices should have alpha = 1.0
    for v in &verts {
        assert_eq!(v[7], 1.0);
    }
    // Check UV corners appear in the vertices
    // First vertex should be top-left
    assert_eq!(verts[0][0], 0.0); // x0
    assert_eq!(verts[0][1], 0.0); // y0
    assert_eq!(verts[0][2], 0.0); // uv_left
    assert_eq!(verts[0][3], 0.0); // uv_top
}

#[test]
fn test_make_quad_vertices_zero_alpha() {
    let verts = make_quad_vertices(10.0, 20.0, 30.0, 40.0, 0.0, 0.0, 1.0, 1.0, 0.0);
    for v in &verts {
        assert_eq!(v[7], 0.0, "Alpha should be 0.0");
    }
}

#[test]
fn test_make_quad_vertices_half_alpha() {
    let verts = make_quad_vertices(0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.5);
    for v in &verts {
        assert!((v[7] - 0.5).abs() < f32::EPSILON);
    }
}

// ── Noise additional tests ───────────────────────────────────────────

#[test]
fn test_noise2d_range() {
    // noise2d returns fract() of (sin(...) * large_number).
    // Rust's fract() preserves sign: (-1.3f32).fract() == -0.3
    // So the output range is (-1, 1), not [0, 1).
    for i in 0..100 {
        for j in 0..100 {
            let val = noise2d(i as f32 * 0.7, j as f32 * 1.3);
            assert!(val > -1.0, "noise2d({}, {}) = {} <= -1", i, j, val);
            assert!(val < 1.0, "noise2d({}, {}) = {} >= 1", i, j, val);
        }
    }
}

#[test]
fn test_smooth_noise2d_deterministic() {
    let a = smooth_noise2d(1.5, 2.7);
    let b = smooth_noise2d(1.5, 2.7);
    assert_eq!(a, b);
}

#[test]
fn test_smooth_noise2d_range() {
    // smooth_noise2d bilinearly interpolates noise2d values which
    // are in (-1, 1) due to Rust's fract() preserving sign.
    // Interpolation keeps the result within the same range.
    for i in 0..50 {
        for j in 0..50 {
            let val = smooth_noise2d(i as f32 * 0.3, j as f32 * 0.3);
            assert!(
                val > -1.0 && val < 1.0,
                "smooth_noise2d out of range: {}",
                val
            );
        }
    }
}

// ── Effect deformation tests ─────────────────────────────────────────

#[test]
fn test_wobbly_deform_at_full_progress() {
    // At eased_t=1.0, damping=0, so deformation should be zero
    let (x, y) = wobbly_deform(5, 10, 0.5, 1.0, 1.0, 20.0);
    assert!(
        x.abs() < 0.001,
        "Wobbly x should be ~0 at full progress, got {}",
        x
    );
    assert_eq!(y, 0.0);
}

#[test]
fn test_wobbly_deform_scroll_direction() {
    // Different scroll directions should produce different deformations
    let (x_down, _) = wobbly_deform(2, 10, 0.3, 0.5, 1.0, 20.0);
    let (x_up, _) = wobbly_deform(2, 10, 0.3, 0.5, -1.0, 20.0);
    // strip_t differs based on direction, so x offsets should differ
    assert_ne!(x_down, x_up);
}

#[test]
fn test_wave_deform_at_full_progress() {
    // At eased_t=1.0, damping=0, so wave should be zero
    let (x, y) = wave_deform(3, 10, 0.5, 1.0, 0.5, 15.0, 2.0);
    assert!(
        x.abs() < 0.001,
        "Wave x should be ~0 at full progress, got {}",
        x
    );
    assert_eq!(y, 0.0);
}

#[test]
fn test_liquid_deform_at_full_progress() {
    // At eased_t=1.0, damping=0, so liquid should be zero
    let (x, y) = liquid_deform(2, 10, 0.5, 1.0, 1.0, 30.0);
    assert!(
        x.abs() < 0.001,
        "Liquid x should be ~0 at full progress, got {}",
        x
    );
    assert!(
        y.abs() < 0.001,
        "Liquid y should be ~0 at full progress, got {}",
        y
    );
}

#[test]
fn test_tilt_y_offset_center_is_zero() {
    // At t=0.5 (center), centered = 0, so offset should be 0
    let offset = tilt_y_offset(0.5, 1.0, 10.0);
    assert!(
        offset.abs() < 0.001,
        "Tilt at center should be ~0, got {}",
        offset
    );
}

#[test]
fn test_tilt_y_offset_symmetry() {
    // Tilt should be symmetric around center: offset(t) = -offset(1-t)
    let top = tilt_y_offset(0.0, 1.0, 10.0);
    let bottom = tilt_y_offset(1.0, 1.0, 10.0);
    assert!(
        (top + bottom).abs() < 0.001,
        "Tilt should be antisymmetric: top={}, bottom={}",
        top,
        bottom
    );
}

#[test]
fn test_tilt_y_offset_zero_velocity() {
    // With zero velocity factor, no tilt
    let offset = tilt_y_offset(0.0, 0.0, 100.0);
    assert_eq!(offset, 0.0);
}

#[test]
fn test_cylinder_roll_scale_always_positive() {
    for i in 0..=10 {
        let t = i as f32 / 10.0;
        let (_, _, scale) = cylinder_roll_transform(t, 0.5, 1.0, 800.0);
        assert!(
            scale > 0.0,
            "Cylinder scale should be > 0 at t={}, got {}",
            t,
            scale
        );
        assert!(
            scale >= 0.3,
            "Cylinder scale should be >= 0.3 at t={}, got {}",
            t,
            scale
        );
    }
}

#[test]
fn test_page_curl_above_curl_line_is_flat() {
    // curl_progress=0.5 → curl_y=0.5
    // At t=0.2 (above curl), should be flat
    let (x, y, alpha) = page_curl_transform(0.2, 0.5, 400.0);
    assert_eq!(x, 0.0);
    assert_eq!(y, 0.0);
    assert_eq!(alpha, 1.0);
}

#[test]
fn test_page_curl_below_curl_line_is_deformed() {
    // curl_progress=0.5 → curl_y=0.5
    // At t=0.8 (below curl), should have deformation
    let (_x, y, alpha) = page_curl_transform(0.8, 0.5, 400.0);
    // y_offset should be negative (curling away)
    assert!(y < 0.0, "Page curl y should be negative, got {}", y);
    // Alpha should be reduced (darkened backside)
    assert!(
        alpha < 1.0,
        "Page curl alpha should be < 1.0, got {}",
        alpha
    );
    assert!(
        alpha >= 0.2,
        "Page curl alpha should be >= 0.2, got {}",
        alpha
    );
}

#[test]
fn test_card_flip_midpoint_minimal_scale() {
    // At t=0.5, angle=PI/2, cos(PI/2)=0, so scale_y should be at minimum (0.02)
    let (scale_y, _) = card_flip_transform(0.5);
    assert!(
        scale_y < 0.1,
        "Card flip scale at midpoint should be near minimum, got {}",
        scale_y
    );
}

#[test]
fn test_card_flip_alpha_halves() {
    // First half: alpha=1.0 (show old content)
    let (_, alpha_start) = card_flip_transform(0.0);
    assert_eq!(alpha_start, 1.0);
    let (_, alpha_quarter) = card_flip_transform(0.25);
    assert_eq!(alpha_quarter, 1.0);
    // Second half: alpha=0.0 (show new content)
    let (_, alpha_three_quarter) = card_flip_transform(0.75);
    assert_eq!(alpha_three_quarter, 0.0);
    let (_, alpha_end) = card_flip_transform(1.0);
    assert_eq!(alpha_end, 0.0);
}

#[test]
fn test_card_flip_scale_at_boundaries() {
    // At t=0, angle=0, cos(0)=1, scale_y should be ~1.0
    let (scale_y_start, _) = card_flip_transform(0.0);
    assert!(
        (scale_y_start - 1.0).abs() < 0.01,
        "Card flip scale at start should be ~1.0, got {}",
        scale_y_start
    );
    // At t=1, angle=PI, cos(PI)=-1, abs=1, scale_y should be ~1.0
    let (scale_y_end, _) = card_flip_transform(1.0);
    assert!(
        (scale_y_end - 1.0).abs() < 0.01,
        "Card flip scale at end should be ~1.0, got {}",
        scale_y_end
    );
}

// ── PostProcessParams additional tests ───────────────────────────────

#[test]
fn test_post_process_params_default_all_zero() {
    let p = PostProcessParams::default();
    assert_eq!(p.scroll_velocity, 0.0);
    assert_eq!(p.scroll_speed, 0.0);
    assert_eq!(p.scroll_direction, 0.0);
    assert_eq!(p.scroll_position, 0.0);
    assert_eq!(p.time, 0.0);
    // All derived values should also be zero or near-zero
    assert_eq!(p.motion_blur_offset(), 0.0);
    assert_eq!(p.chromatic_offset(), 0.0);
    assert_eq!(p.ghost_opacity(), 0.0);
    assert_eq!(p.color_temp_shift(), 0.0);
    assert_eq!(p.scanline_phase(), 0.0);
    assert_eq!(p.dof_blur_radius(), 0.0);
}

#[test]
fn test_post_process_params_max_clamping() {
    let p = PostProcessParams {
        scroll_velocity: 10000.0,
        scroll_speed: 100.0, // Very high speed
        scroll_direction: 1.0,
        scroll_position: 9999.0,
        time: 100.0,
    };
    // Motion blur capped at 12.0
    assert!((p.motion_blur_offset() - 12.0).abs() < 0.001);
    // Chromatic offset capped at 5.0
    assert!((p.chromatic_offset() - 5.0).abs() < 0.001);
    // Ghost opacity capped at 0.25
    assert!((p.ghost_opacity() - 0.25).abs() < 0.001);
    // DOF blur radius capped at 6.0
    assert!((p.dof_blur_radius() - 6.0).abs() < 0.001);
}

#[test]
fn test_post_process_color_temp_direction() {
    let p_down = PostProcessParams {
        scroll_speed: 0.5,
        scroll_direction: 1.0,
        ..Default::default()
    };
    let p_up = PostProcessParams {
        scroll_speed: 0.5,
        scroll_direction: -1.0,
        ..Default::default()
    };
    // Scrolling down = warm (positive), scrolling up = cool (negative)
    assert!(p_down.color_temp_shift() > 0.0);
    assert!(p_up.color_temp_shift() < 0.0);
    // Magnitudes should be equal
    assert!((p_down.color_temp_shift().abs() - p_up.color_temp_shift().abs()).abs() < 0.0001);
}

#[test]
fn test_post_process_scanline_phase_proportional() {
    let p1 = PostProcessParams {
        scroll_position: 100.0,
        ..Default::default()
    };
    let p2 = PostProcessParams {
        scroll_position: 200.0,
        ..Default::default()
    };
    // Phase should be proportional to position
    assert!((p2.scanline_phase() - 2.0 * p1.scanline_phase()).abs() < 0.001);
}
