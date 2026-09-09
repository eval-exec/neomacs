//! Real GPU pixels for the native content-placement seam; no AppKit required.
use neomacs_display_protocol::{Color, ContentInsets, DeviceScale, DrawableSurface};
use neomacs_renderer_wgpu::{SnapshotSize, WgpuRenderer, renderer::RenderTarget};
use std::{num::NonZeroU32, sync::Arc, time::Duration};

#[test]
fn native_titlebar_background_preserves_content_pixels_and_alpha() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("SKIP: no GPU adapter for native content-placement readback");
        return;
    };
    eprintln!("native-content test adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(neomacs_renderer_wgpu::request_renderer_device(
        &adapter,
        "native chrome test",
    ))
    .unwrap();
    let (device, queue) = (Arc::new(device), Arc::new(queue));
    let mut renderer = WgpuRenderer::with_device(
        device.clone(),
        queue.clone(),
        4,
        4,
        wgpu::TextureFormat::Rgba8Unorm,
        1.0,
    );
    let source = renderer
        .acquire_snapshot(SnapshotSize::new(4, 2).unwrap())
        .unwrap();
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("native chrome output"),
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
    let view = output.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("translucent editor pixels"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: source.view(),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.5,
                        b: 0.0,
                        a: 0.5,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    queue.submit([encoder.finish()]);
    let surface = DrawableSurface::new(
        NonZeroU32::new(4).unwrap(),
        NonZeroU32::new(4).unwrap(),
        DeviceScale::new(1.0).unwrap(),
    )
    .unwrap()
    .with_content_insets(ContentInsets::new(0, 2, 0, 0));
    let placement = neomacs_renderer_wgpu::renderer::NativeContentPlacement::new(
        RenderTarget::new(&view, surface),
        &source,
    )
    .unwrap();
    renderer.place_native_content(placement, Color::new(1.0, 0.0, 1.0, 0.5));
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("native chrome readback"),
        size: 256 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        output.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(4),
            },
        },
        output.size(),
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
    let pixels = slice.get_mapped_range().unwrap();
    for y in 0..4 {
        for x in 0..4 {
            let expected = if y < 2 {
                [128, 0, 128, 128]
            } else {
                [0, 128, 0, 128]
            };
            assert_eq!(
                &pixels[y * 256 + x * 4..y * 256 + x * 4 + 4],
                &expected,
                "pixel ({x}, {y}) must be placed without stretching or double alpha blending"
            );
        }
    }
}
