//! Real device creation and drawing on the compute-less GL profile from #648.
//!
//! Run explicitly with Mesa's software driver (the display CI job does this):
//! `LIBGL_ALWAYS_SOFTWARE=1 EGL_PLATFORM=surfaceless MESA_GL_VERSION_OVERRIDE=3.3
//! MESA_GLES_VERSION_OVERRIDE=3.0 MESA_GLSL_VERSION_OVERRIDE=330
//! MESA_EXTENSION_OVERRIDE=-GL_ARB_compute_shader cargo nextest run
//! -p neomacs-renderer-wgpu --run-ignored only -E 'test(compute_less_gl)'`.
//!
//! GLVND hosts with multiple vendors may also need
//! `__EGL_VENDOR_LIBRARY_FILENAMES` set to their Mesa `50_mesa.json`.
//! Missing GL or nonzero compute support is an error when this test is run,
//! rather than a successful test that never exercises the reported failure.

use neomacs_display_protocol::{
    Color, DisplayWindowId, Rect,
    scene::{Scene, WindowScene},
};
use neomacs_renderer_wgpu::{WgpuGlyphAtlas, WgpuRenderer, request_renderer_device};
use std::{sync::Arc, time::Duration};

#[test]
#[ignore = "requires compute-less GL; display CI runs it with Mesa overrides"]
fn compute_less_gl_renderer_draws_and_reads_back_pixels() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends = wgpu::Backends::GL;
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("compute-less GL adapter is required; see this test's Mesa invocation");
    let info = adapter.get_info();
    eprintln!("compute-less GL adapter: {info:?}");
    assert_eq!(info.backend, wgpu::Backend::Gl);
    assert_eq!(
        adapter.limits().max_compute_workgroups_per_dimension,
        0,
        "this regression requires a GL adapter without compute support: {info:?}"
    );

    let (device, queue) =
        pollster::block_on(request_renderer_device(&adapter, "downlevel GL test"))
            .expect("renderer device request must fit the compute-less adapter");
    let (device, queue) = (Arc::new(device), Arc::new(queue));
    let renderer = WgpuRenderer::with_device(
        device.clone(),
        queue.clone(),
        4,
        4,
        wgpu::TextureFormat::Rgba8Unorm,
        1.0,
    );
    let _atlas = WgpuGlyphAtlas::new(&device);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("downlevel GL target"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut scene = Scene::new(4.0, 4.0);
    scene.background = Color::RED;
    scene.windows.push(WindowScene {
        window_id: DisplayWindowId::new(1),
        bounds: Rect::new(0.0, 0.0, 4.0, 4.0),
        background: Color::GREEN,
        cursor: None,
        scroll_offset: 0.0,
        selected: true,
        mode_line_height: 0,
        header_line_height: 0,
        last_frame_touched: 0,
    });
    renderer.render_to_view(&target.create_view(&Default::default()), &scene);

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("downlevel GL readback"),
        size: 256 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(4),
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| {
        result.expect("map GL pixel readback")
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("finish GL drawing and readback");
    let pixels = slice
        .get_mapped_range()
        .expect("GL readback must be mapped");
    assert_eq!(
        &pixels[..4],
        &[0, 255, 0, 255],
        "the renderer must draw green over the red clear color"
    );
}
