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
