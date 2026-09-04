use super::*;

// =======================================================================
// MotionDuration: zero is unrepresentable so samplers cannot divide by it
// =======================================================================

#[test]
fn motion_duration_rejects_zero() {
    assert_eq!(
        MotionDuration::new(Duration::ZERO),
        Err(MotionSpecError::ZeroDuration)
    );
    assert_eq!(
        MotionDuration::from_secs_f32(0.0),
        Err(MotionSpecError::NotPositive)
    );
}

#[test]
fn motion_duration_rejects_negative_and_non_finite_seconds() {
    assert_eq!(
        MotionDuration::from_secs_f32(-0.25),
        Err(MotionSpecError::NotPositive)
    );
    assert_eq!(
        MotionDuration::from_secs_f32(f32::NAN),
        Err(MotionSpecError::NotFinite)
    );
    assert_eq!(
        MotionDuration::from_secs_f32(f32::INFINITY),
        Err(MotionSpecError::NotFinite)
    );
}

#[test]
fn motion_duration_round_trips_seconds() {
    let duration = MotionDuration::from_secs_f32(0.15).expect("positive");
    assert!((duration.as_secs_f32() - 0.15).abs() < 1e-6);
    assert_eq!(duration.get(), Duration::from_secs_f32(0.15));
}

// =======================================================================
// UnitInterval: clamp is total (NaN-safe), new is checked
// =======================================================================

#[test]
fn unit_interval_clamp_saturates_and_is_nan_safe() {
    assert_eq!(UnitInterval::clamp(-1.0), UnitInterval::ZERO);
    assert_eq!(UnitInterval::clamp(2.0), UnitInterval::ONE);
    assert_eq!(UnitInterval::clamp(0.25).get(), 0.25);
    // A sampler that went unstable must render un-progressed, not emit NaN.
    assert_eq!(UnitInterval::clamp(f32::NAN), UnitInterval::ZERO);
    assert_eq!(UnitInterval::clamp(f32::INFINITY), UnitInterval::ONE);
    assert_eq!(UnitInterval::clamp(f32::NEG_INFINITY), UnitInterval::ZERO);
}

#[test]
fn unit_interval_new_rejects_non_finite_but_saturates_finite() {
    assert_eq!(UnitInterval::new(f32::NAN), Err(MotionSpecError::NotFinite));
    assert_eq!(
        UnitInterval::new(f32::INFINITY),
        Err(MotionSpecError::NotFinite)
    );
    assert_eq!(UnitInterval::new(1.5).expect("finite"), UnitInterval::ONE);
    assert_eq!(UnitInterval::new(-0.5).expect("finite"), UnitInterval::ZERO);
}

#[test]
fn unit_interval_clamp_never_yields_a_value_outside_the_unit_range() {
    for raw in [
        f32::MIN,
        -1e9,
        -1.0,
        -f32::EPSILON,
        0.0,
        0.5,
        1.0,
        1.0 + f32::EPSILON,
        1e9,
        f32::MAX,
    ] {
        let value = UnitInterval::clamp(raw).get();
        assert!(
            (0.0..=1.0).contains(&value),
            "clamp({raw}) produced {value}"
        );
    }
}

// =======================================================================
// Spring parameters: a constructed spring always converges
// =======================================================================

#[test]
fn angular_frequency_and_damping_reject_non_positive_and_non_finite() {
    for bad in [0.0f32, -1.0] {
        assert_eq!(
            AngularFrequency::new(bad),
            Err(MotionSpecError::NotPositive)
        );
        assert_eq!(DampingRatio::new(bad), Err(MotionSpecError::NotPositive));
    }
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(AngularFrequency::new(bad), Err(MotionSpecError::NotFinite));
        assert_eq!(DampingRatio::new(bad), Err(MotionSpecError::NotFinite));
    }
    assert_eq!(AngularFrequency::new(12.0).expect("positive").get(), 12.0);
    assert_eq!(DampingRatio::new(1.0).expect("positive").get(), 1.0);
}

// =======================================================================
// DevicePixels: a threshold may be zero but never negative
// =======================================================================

#[test]
fn device_pixels_allows_zero_but_rejects_negative_and_non_finite() {
    assert_eq!(DevicePixels::new(0.0).expect("zero ok"), DevicePixels::ZERO);
    assert_eq!(DevicePixels::new(-0.5), Err(MotionSpecError::Negative));
    assert_eq!(DevicePixels::new(f32::NAN), Err(MotionSpecError::NotFinite));
    assert_eq!(DevicePixels::new(2.5).expect("positive").get(), 2.5);
}

// =======================================================================
// MotionSpec
// =======================================================================

#[test]
fn motion_spec_defaults_to_instant_so_absent_config_never_animates() {
    assert_eq!(MotionSpec::default(), MotionSpec::Instant);
    assert!(MotionSpec::default().is_instant());
}

#[test]
fn only_the_instant_variant_reports_is_instant() {
    let tween = MotionSpec::Tween(TweenSpec {
        duration: MotionDuration::from_secs_f32(0.2).expect("positive"),
        easing: TransitionEasing::EaseOutQuad,
    });
    let spring = MotionSpec::Spring(SpringSpec {
        omega: AngularFrequency::new(20.0).expect("positive"),
        damping: DampingRatio::new(1.0).expect("positive"),
    });
    let decel = MotionSpec::Deceleration(DecelerationSpec {
        friction: AngularFrequency::new(4.0).expect("positive"),
    });
    assert!(!tween.is_instant());
    assert!(!spring.is_instant());
    assert!(!decel.is_instant());
    assert!(MotionSpec::Instant.is_instant());
}

#[test]
fn motion_spec_names_are_stable_for_diagnostics() {
    let name: &'static str = MotionSpec::Instant.into();
    assert_eq!(name, "instant");
    let name: &'static str = MotionSpec::Spring(SpringSpec {
        omega: AngularFrequency::new(1.0).expect("positive"),
        damping: DampingRatio::new(1.0).expect("positive"),
    })
    .into();
    assert_eq!(name, "spring");
}

// =======================================================================
// Serde: deserialization validates, so an invalid config cannot be loaded
// =======================================================================

#[test]
fn deserializing_rejects_values_the_constructors_reject() {
    assert!(serde_json::from_str::<AngularFrequency>("0.0").is_err());
    assert!(serde_json::from_str::<AngularFrequency>("-3.0").is_err());
    assert!(serde_json::from_str::<DampingRatio>("0.0").is_err());
    assert!(serde_json::from_str::<DevicePixels>("-1.0").is_err());
    assert!(serde_json::from_str::<AngularFrequency>("8.0").is_ok());
    assert!(serde_json::from_str::<DevicePixels>("0.0").is_ok());
}

#[test]
fn unit_interval_deserialization_saturates_finite_values() {
    let over: UnitInterval = serde_json::from_str("1.75").expect("finite");
    assert_eq!(over, UnitInterval::ONE);
    let under: UnitInterval = serde_json::from_str("-0.25").expect("finite");
    assert_eq!(under, UnitInterval::ZERO);
}

#[test]
fn motion_spec_round_trips_through_json() {
    let spec = MotionSpec::Tween(TweenSpec {
        duration: MotionDuration::from_secs_f32(0.3).expect("positive"),
        easing: TransitionEasing::Spring,
    });
    let json = serde_json::to_string(&spec).expect("serialize");
    let back: MotionSpec = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(spec, back);
}

#[test]
fn newtypes_serialize_transparently_as_bare_numbers() {
    assert_eq!(
        serde_json::to_string(&AngularFrequency::new(12.5).expect("positive")).expect("serialize"),
        "12.5"
    );
    assert_eq!(
        serde_json::to_string(&UnitInterval::clamp(0.5)).expect("serialize"),
        "0.5"
    );
}
