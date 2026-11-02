use super::requested_limits;

/// Representative downlevel OpenGL limits. The zero compute fields model
/// #648; the uniform count exercises another below-WebGPU limit without
/// claiming it was measured on the reporter's adapter.
fn downlevel_gl_limits() -> wgpu::Limits {
    wgpu::Limits {
        max_compute_workgroup_storage_size: 0,
        max_compute_invocations_per_workgroup: 0,
        max_compute_workgroup_size_x: 0,
        max_compute_workgroup_size_y: 0,
        max_compute_workgroup_size_z: 0,
        max_compute_workgroups_per_dimension: 0,
        max_uniform_buffers_per_shader_stage: 11,
        ..wgpu::Limits::default()
    }
}

#[test]
fn webgpu_defaults_do_not_fit_a_downlevel_adapter() {
    let allowed = downlevel_gl_limits();
    // The pre-clamp request rejected the device before it could start.
    assert!(
        !wgpu::Limits::default()
            .using_resolution(allowed.clone())
            .check_limits(&allowed)
    );
}

#[test]
fn requested_limits_fit_a_downlevel_adapter() {
    let allowed = downlevel_gl_limits();
    let requested = requested_limits(&allowed);

    assert!(requested.check_limits(&allowed));
    assert_eq!(requested.max_compute_workgroups_per_dimension, 0);
    assert_eq!(requested.max_uniform_buffers_per_shader_stage, 11);
}

#[test]
fn requested_limits_keep_adapter_resolution_and_stricter_alignment() {
    let allowed = wgpu::Limits {
        max_texture_dimension_2d: 16384,
        min_uniform_buffer_offset_alignment: 512,
        ..wgpu::Limits::default()
    };
    let requested = requested_limits(&allowed);

    assert_eq!(requested.max_texture_dimension_2d, 16384);
    assert_eq!(requested.min_uniform_buffer_offset_alignment, 512);
    assert!(requested.check_limits(&allowed));
}

#[test]
fn requested_limits_are_the_defaults_on_a_capable_adapter() {
    let allowed = wgpu::Limits::default();
    assert_eq!(requested_limits(&allowed), wgpu::Limits::default());
}
