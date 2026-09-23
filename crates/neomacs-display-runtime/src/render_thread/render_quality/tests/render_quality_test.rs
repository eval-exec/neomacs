use super::*;

#[test]
fn backend_profile_projects_wgpu_device_cost_class_once() {
    assert_eq!(
        RenderBackendProfile::from_device_type(wgpu::DeviceType::Cpu).adapter_class(),
        AdapterClass::Software
    );
    for device_type in [
        wgpu::DeviceType::Other,
        wgpu::DeviceType::IntegratedGpu,
        wgpu::DeviceType::DiscreteGpu,
        wgpu::DeviceType::VirtualGpu,
    ] {
        assert_eq!(
            RenderBackendProfile::from_device_type(device_type).adapter_class(),
            AdapterClass::Hardware
        );
    }
}

#[test]
fn software_compat_policy_preserves_request_and_owns_every_feature_decision() {
    let mut requested = VisualConfig::default();
    requested.cursor_motion.enabled = true;
    requested.cursor_size_transition.enabled = true;
    requested.buffer_transition.enabled = true;
    requested.scroll_transition.enabled = true;
    requested.effects.cursor_glow.enabled = true;
    requested.effects.bg_pattern.style = 1;

    let policy = RenderQualityPolicy::negotiate(RenderBackendProfile::software(), &requested);

    assert_eq!(policy.mode(), QualityMode::SoftwareCompatibility);
    assert_eq!(
        policy.dynamic_animation_rate(NonZeroU16::new(144).unwrap()),
        None,
        "software compatibility has a hard zero-cadence bound"
    );
    assert!(!policy.effective_visual_config().cursor_motion.enabled);
    assert!(!policy.effective_visual_config().effects.cursor_glow.enabled);
    assert_eq!(requested.effects.bg_pattern.style, 1);
    assert_eq!(
        policy.frame_post_disposition(),
        FramePostDisposition::SuppressedByQualityPolicy
    );
    assert!(!policy.frame_post_scheduler_active(true, true));
    assert_eq!(
        policy.plan_frame(true, true),
        RenderFeaturePlan {
            compose_offscreen: false,
            accept_transition_hints: false,
            accept_derived_effects: false,
            accept_cursor_effects: false,
            apply_frame_post: false,
        }
    );
}

#[test]
fn full_policy_preserves_requested_features() {
    let requested = VisualConfig::default();
    let policy = RenderQualityPolicy::negotiate(RenderBackendProfile::hardware(), &requested);

    assert_eq!(policy.mode(), QualityMode::Full);
    assert_eq!(policy.effective_visual_config(), &requested);
    assert_eq!(
        policy.frame_post_disposition(),
        FramePostDisposition::Enabled
    );
    assert!(policy.frame_post_scheduler_active(true, true));
    assert!(
        !policy.frame_post_scheduler_active(true, false),
        "an awaiting-content window must not inherit global shader demand"
    );
    assert_eq!(
        policy.dynamic_animation_rate(NonZeroU16::new(144).unwrap()),
        NonZeroU16::new(144)
    );
}
