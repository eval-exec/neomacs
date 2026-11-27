//! Real device creation and drawing on the compute-less GL profile from #648.
//!
//! Runs directly when the caller already configured Mesa's compute-less
//! software OpenGL 3.3 overrides (`MESA_EXTENSION_OVERRIDE` containing
//! `-GL_ARB_compute_shader`), and otherwise re-executes this single test binary
//! in a child process with those Mesa overrides applied so ordinary
//! `cargo nextest run -p neomacs-renderer-wgpu` runs exercise it without
//! skipping.
//!
//! Missing Mesa software OpenGL (`libegl1`, `libegl-mesa0`, `libgl1-mesa-dri`)
//! or nonzero compute support fails closed with a diagnostic error rather than
//! silently skipping.

use neomacs_display_protocol::{
    Color, DisplayWindowId, Rect,
    scene::{Scene, WindowScene},
};
use neomacs_renderer_wgpu::{WgpuGlyphAtlas, WgpuRenderer, request_renderer_device};
use std::{path::Path, process::Command, sync::Arc, time::Duration};

const CHILD_ENV: &str = "NEOMACS_DOWNLEVEL_GL_CHILD";
const TEST_NAME: &str = "downlevel_gl::compute_less_gl_renderer_draws_and_reads_back_pixels";
const VERIFIED_MARKER: &str = "COMPUTE_LESS_GL_VERIFIED";
const MESA_EGL_VENDOR_CANDIDATES: &[&str] = &[
    "/run/opengl-driver/share/glvnd/egl_vendor.d/50_mesa.json",
    "/usr/share/glvnd/egl_vendor.d/50_mesa.json",
    "/etc/glvnd/egl_vendor.d/50_mesa.json",
];

fn has_compute_less_mesa_env() -> bool {
    std::env::var_os(CHILD_ENV).is_some()
        || std::env::var("MESA_EXTENSION_OVERRIDE")
            .ok()
            .is_some_and(|v| v.contains("-GL_ARB_compute_shader"))
}

fn find_mesa_egl_vendor_json() -> Option<&'static str> {
    MESA_EGL_VENDOR_CANDIDATES
        .iter()
        .copied()
        .find(|path| Path::new(path).is_file())
}

#[test]
fn compute_less_gl_renderer_draws_and_reads_back_pixels() {
    if !has_compute_less_mesa_env() {
        let exe = std::env::current_exe().expect("locate current test executable");
        let mut cmd = Command::new(&exe);
        cmd.args(["--exact", TEST_NAME, "--nocapture"])
            .env(CHILD_ENV, "1")
            .env("LIBGL_ALWAYS_SOFTWARE", "1")
            .env("EGL_PLATFORM", "surfaceless")
            .env("MESA_GL_VERSION_OVERRIDE", "3.3")
            .env("MESA_GLES_VERSION_OVERRIDE", "3.0")
            .env("MESA_GLSL_VERSION_OVERRIDE", "330")
            .env("MESA_EXTENSION_OVERRIDE", "-GL_ARB_compute_shader");
        if std::env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").is_none()
            && let Some(mesa_json) = find_mesa_egl_vendor_json()
        {
            cmd.env("__EGL_VENDOR_LIBRARY_FILENAMES", mesa_json);
        }
        let output = cmd.output().expect("spawn compute-less GL child test");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stdout.contains(VERIFIED_MARKER),
            "compute-less Mesa OpenGL 3.3 test failed in child process ({}).\n\
             Prerequisite: Mesa software EGL/GL runtime (`libegl1`, `libegl-mesa0`, `libgl1-mesa-dri`; \
             on multi-vendor GLVND hosts set `__EGL_VENDOR_LIBRARY_FILENAMES` to `50_mesa.json`).\n\
             stdout:\n{stdout}\nstderr:\n{stderr}",
            output.status,
        );
        return;
    }

    run_compute_less_gl_draw_and_readback();
}

fn run_compute_less_gl_draw_and_readback() {
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
    println!("{VERIFIED_MARKER}");
}
