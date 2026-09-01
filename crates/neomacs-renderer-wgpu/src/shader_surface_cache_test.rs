//! Tests for `ShaderSurfaceCache`, focused on DPI rescale on monitor change.

use super::ShaderSurfaceCache;
use crate::image_cache::ImageCache;
use crate::shader_surface::SurfaceShaderLanguage;

#[test]
fn clamp_size_is_a_pure_function_of_logical_size_and_scale() {
    // Physical size is round(logical * scale), clamped to [1, MAX].
    assert_eq!(ShaderSurfaceCache::clamp_size(100, 50, 1.0), (100, 50));
    assert_eq!(ShaderSurfaceCache::clamp_size(100, 50, 2.0), (200, 100));
    assert_eq!(ShaderSurfaceCache::clamp_size(100, 50, 1.25), (125, 63)); // 62.5 rounds to 63
    // Non-finite / non-positive scale falls back to 1.0.
    assert_eq!(ShaderSurfaceCache::clamp_size(100, 50, 0.0), (100, 50));
    assert_eq!(ShaderSurfaceCache::clamp_size(100, 50, f32::NAN), (100, 50));
    // Floor is 1px, never 0.
    assert_eq!(ShaderSurfaceCache::clamp_size(1, 1, 0.0001), (1, 1));

    // Drift-freedom: because physical size is a pure function of the RETAINED
    // logical size (never of the previous physical size), visiting scale 2.0
    // and returning to 1.0 lands back on the exact original — which a
    // physical/old_scale round-trip (rounding twice) would not guarantee.
    let (lw, lh) = (137u32, 89u32);
    let at_1 = ShaderSurfaceCache::clamp_size(lw, lh, 1.0);
    let _at_2 = ShaderSurfaceCache::clamp_size(lw, lh, 2.0);
    let back_to_1 = ShaderSurfaceCache::clamp_size(lw, lh, 1.0);
    assert_eq!(at_1, back_to_1);
    assert_eq!(at_1, (137, 89));
}

fn try_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("shader-surface-rescale-test"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: Default::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        trace: wgpu::Trace::Off,
    }))
    .ok()
}

const TRIVIAL_WGSL: &str =
    "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.0); }";

#[test]
fn rescale_resamples_shader_surfaces_and_round_trips_without_drift() {
    let Some((device, _queue)) = try_device() else {
        eprintln!("skipping: no headless wgpu adapter");
        return;
    };
    let image = ImageCache::new(&device);
    let mut cache = ShaderSurfaceCache::new(&device);
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;

    // A 200x120 logical shader surface created at scale 1.0.
    let created = cache.create_shader(
        &device,
        image.bind_group_layout(),
        image.sampler(),
        format,
        1,
        SurfaceShaderLanguage::Wgsl,
        TRIVIAL_WGSL,
        &[],
        200,
        120,
        1.0,
        true,
        None, // fps: uncapped
        None, // channel0
    );
    let Ok((w0, h0)) = created else {
        eprintln!("skipping: device rejected the trivial pipeline: {created:?}");
        return;
    };
    assert_eq!((w0, h0), (200, 120));

    // Scale 2.0: physical dims double; the changed surface is reported once.
    let up = cache.rescale(
        &device,
        image.bind_group_layout(),
        image.sampler(),
        format,
        2.0,
    );
    assert_eq!(up, vec![(1, 400, 240)]);

    // Re-applying the same scale is a no-op (nothing reported).
    let again = cache.rescale(
        &device,
        image.bind_group_layout(),
        image.sampler(),
        format,
        2.0,
    );
    assert!(again.is_empty(), "same-scale rescale should report nothing");

    // Back to 1.0 lands EXACTLY on the original physical size (drift-free).
    let down = cache.rescale(
        &device,
        image.bind_group_layout(),
        image.sampler(),
        format,
        1.0,
    );
    assert_eq!(down, vec![(1, 200, 120)]);
}

#[test]
fn rescale_leaves_pixel_surfaces_untouched() {
    let Some((device, queue)) = try_device() else {
        eprintln!("skipping: no headless wgpu adapter");
        return;
    };
    let image = ImageCache::new(&device);
    let mut cache = ShaderSurfaceCache::new(&device);
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;

    // A 4x4 RGBA pixel surface (content-defined, no shader pipeline).
    let data = vec![0u8; 4 * 4 * 4];
    cache
        .create_pixels(
            &device,
            &queue,
            image.bind_group_layout(),
            image.sampler(),
            7,
            &data,
            4,
            4,
        )
        .expect("create pixel surface");

    // Pixel surfaces carry their own fixed-resolution data, so DPI rescale
    // must not touch them (nothing reported).
    let rescaled = cache.rescale(
        &device,
        image.bind_group_layout(),
        image.sampler(),
        format,
        2.0,
    );
    assert!(
        rescaled.is_empty(),
        "pixel surfaces must be skipped by DPI rescale, got {rescaled:?}"
    );
}

#[test]
fn active_animation_max_fps_takes_max_cap_and_uncapped_forces_full_rate() {
    let Some((device, _queue)) = try_device() else {
        eprintln!("skipping: no headless wgpu adapter");
        return;
    };
    let image = ImageCache::new(&device);
    let mut cache = ShaderSurfaceCache::new(&device);
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let make = |cache: &mut ShaderSurfaceCache, id: u32, fps: Option<u32>| -> bool {
        cache
            .create_shader(
                &device,
                image.bind_group_layout(),
                image.sampler(),
                format,
                id,
                SurfaceShaderLanguage::Wgsl,
                TRIVIAL_WGSL,
                &[],
                64,
                64,
                1.0,
                true,
                fps,
                None,
            )
            .is_ok()
    };

    // Nothing composited yet → no sustained animation demand.
    assert_eq!(cache.active_animation_max_fps(), None);

    // Two capped surfaces, both composited → highest cap wins.
    if !make(&mut cache, 1, Some(10)) {
        eprintln!("skipping: device rejected the trivial pipeline");
        return;
    }
    assert!(make(&mut cache, 2, Some(30)));
    cache.mark_drawn(1);
    cache.mark_drawn(2);
    assert_eq!(cache.active_animation_max_fps(), Some(30));

    // An uncapped active surface forces the full display rate (None).
    assert!(make(&mut cache, 3, None));
    cache.mark_drawn(3);
    assert_eq!(cache.active_animation_max_fps(), None);
}
