//! Headless GPU capability probe.
//!
//! Confirms this environment can create an offscreen wgpu device and read
//! pixels back from a rendered texture — the foundation the offscreen frame
//! render-diff tests need (frame scheduling plan, testing strategy: "compare
//! a static frame plus two cursor-time samples and confirm that only the
//! cursor pixels change"). Independent of any window-system surface, so it
//! runs even where the Wayland/X11 surface handshake is unavailable.
//!
//! Passes (skips) cleanly if no adapter is available, so CI without a GPU is
//! not broken by it.

#[test]
fn offscreen_clear_readback() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        eprintln!("SKIP: no wgpu adapter available in this environment");
        return;
    };
    eprintln!(
        "ADAPTER: {:?} / {:?}",
        adapter.get_info().name,
        adapter.get_info().backend
    );
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("probe"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: Default::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        trace: wgpu::Trace::Off,
    }))
    .expect("device");

    let size = wgpu::Extent3d {
        width: 4,
        height: 4,
        depth_or_array_layers: 1,
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("probe-tex"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let mut enc = device.create_command_encoder(&Default::default());
    {
        let _rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("probe-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 1.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("probe-readback"),
        size: 256 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(4),
            },
        },
        size,
    );
    queue.submit(std::iter::once(enc.finish()));
    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(3)),
        })
        .expect("poll");
    let data = slice
        .get_mapped_range()
        .expect("headless probe readback buffer should remain mapped");
    let px = [data[0], data[1], data[2], data[3]];
    eprintln!("PIXEL: {:?}", px);
    assert_eq!(px, [0, 255, 0, 255], "cleared green pixel should read back");
}
