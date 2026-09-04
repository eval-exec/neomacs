use super::*;
use crate::core::frame_glyphs::FrameGlyphBuffer;
use crate::core::types::Color;
use crate::render_thread::frame_windows::{FrameLifecycle, GuiFrameRenderState};
use crate::render_thread::pointer_events::PointerOwner;
use crate::render_thread::state::{
    ActivePointerAppearance, PointerAppearancePhase, PointerAppearanceState, PointerCursorIntent,
    PresentedAppearanceKey, PresentedInteractionKey, PresentedPressCapture,
};
use neomacs_display_protocol::frame_chrome::InteractionId;
use neomacs_display_protocol::frame_chrome::PresentationId;
use neomacs_display_protocol::{
    Face, FaceId, FrameRect, ImageId, PointerAppearanceId,
    PointerAppearancePhase as ProtocolPointerAppearancePhase, PointerDrawMode, PointerImageRelief,
    PointerReliefCornerErase, PointerReliefEdges, PointerReliefMargins, PresentedHitIndex,
    PresentedHitRegion, PresentedPaintSpan, PresentedPointerAppearance, PresentedPointerRegion,
    PresentedPrimitiveKind, PresentedRegionId, PresentedRegionKind,
};
use winit::keyboard::{Key, NamedKey, SmolStr};
use winit::window::ResizeDirection;

#[test]
fn presented_window_resize_cursor_intent_maps_both_axes_and_has_typed_precedence() {
    use neomacs_display_protocol::{PresentedRegionKind, PresentedResizeAxis};
    use winit::window::CursorIcon;

    assert_eq!(
        PresentedRegionKind::TextBody.resize_axis(),
        None,
        "ordinary presentation regions must not select a resize cursor"
    );
    assert_eq!(
        PointerCursorIntent::resolve(None, PresentedRegionKind::RightDivider.resize_axis(), false,)
            .icon(),
        CursorIcon::EwResize
    );
    assert_eq!(
        PointerCursorIntent::resolve(
            None,
            PresentedRegionKind::BottomDivider.resize_axis(),
            false,
        )
        .icon(),
        CursorIcon::NsResize
    );
    assert_eq!(
        PointerCursorIntent::resolve(
            Some(ResizeDirection::NorthWest),
            Some(PresentedResizeAxis::Horizontal),
            true,
        )
        .icon(),
        CursorIcon::NwResize,
        "native frame-edge resize must take precedence at the outer boundary"
    );
}

/// Build a minimal `RenderApp` suitable for testing `detect_resize_edge`
/// and `titlebar_hit_test`.  Only the fields those methods read are
/// meaningful; everything else is set to harmless defaults.
fn make_test_app(width: u32, height: u32, scale_factor: f64) -> RenderApp {
    make_test_app_with_input(width, height, scale_factor).0
}

fn make_test_app_with_input(
    width: u32,
    height: u32,
    scale_factor: f64,
) -> (RenderApp, crate::thread_comm::EmacsComms) {
    use std::sync::{Arc, Mutex};

    use crate::thread_comm::ThreadComms;

    let comms = ThreadComms::new();
    let (emacs, render) = comms.split();
    let image_metadata = Arc::new(crate::render_thread::ImageRenderState::default());
    let shared_monitors = Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new()));

    let mut app = RenderApp::new(
        render,
        width,
        height,
        "test".to_string(),
        image_metadata,
        shared_monitors,
        true,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    );
    {
        let primary = app.frame_windows.primary_window_mut().unwrap();
        if let FrameLifecycle::Pending {
            scale_factor: sf, ..
        } = &mut primary.lifecycle
        {
            *sf = scale_factor;
        }
        primary.render.set_surface_state(
            neomacs_display_protocol::SurfaceState::from_device_size(
                width,
                height,
                neomacs_display_protocol::DeviceScale::new(scale_factor as f32).unwrap(),
            )
            .unwrap(),
        );
    }
    (app, emacs)
}

#[test]
fn stale_root_presentation_does_not_own_newly_exposed_surface_area() {
    let mut app = make_test_app(200, 100, 1.0);
    let window = app.frame_windows.primary_window_mut().unwrap();
    window.render.set_surface_state(
        neomacs_display_protocol::SurfaceState::from_device_size(
            200,
            100,
            neomacs_display_protocol::DeviceScale::new(1.0).unwrap(),
        )
        .unwrap(),
    );
    let mut stale = FrameGlyphBuffer::with_size(100.0, 50.0);
    stale.presentation_id = PresentationId::new(88);
    window.render.set_current_frame(Some(stale), None);

    assert_eq!(
        RenderApp::pointer_owner(window, 150.0, 25.0),
        PointerOwner::Expose {
            frame_id: 0,
            x: 150.0,
            y: 25.0,
        }
    );
    assert!(matches!(
        RenderApp::pointer_owner(window, 75.0, 25.0),
        PointerOwner::Root {
            frame_id: 0,
            x: 75.0,
            y: 25.0,
        }
    ));
}

fn make_test_device() -> Option<wgpu::Device> {
    let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = wgpu::Backends::all();
    let instance = wgpu::Instance::new(instance_descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .ok()?;
    let (device, _queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("input test device"),
        ..Default::default()
    }))
    .ok()?;
    Some(device)
}

fn test_present_mapping(
    frame: &FrameGlyphBuffer,
    width: u32,
    height: u32,
) -> neomacs_display_protocol::PresentMapping {
    let neomacs_display_protocol::SurfaceState::Drawable(surface) =
        neomacs_display_protocol::SurfaceState::from_device_size(
            width,
            height,
            neomacs_display_protocol::DeviceScale::new(1.0).unwrap(),
        )
        .unwrap()
    else {
        unreachable!("test render target is drawable")
    };
    neomacs_display_protocol::PresentMapping::top_left_clip(
        surface,
        neomacs_display_protocol::PresentationExtent::new(
            frame.presentation_id,
            neomacs_display_protocol::GeometrySize::<neomacs_display_protocol::LogicalPixels>::from_px(
                frame.width,
                frame.height,
            )
            .unwrap(),
        ),
    )
}

fn appearance_key(presentation: u64, appearance: usize) -> PresentedAppearanceKey {
    PresentedAppearanceKey::new(
        PresentationId::new(presentation),
        PointerAppearanceId::try_from(appearance).expect("appearance id"),
    )
}

fn set_test_frame_placement(
    frame: &mut FrameGlyphBuffer,
    frame_id: u64,
    parent_id: u64,
    x: f32,
    y: f32,
    z_order: i32,
) {
    frame.frame_placement = neomacs_display_protocol::PresentedFramePlacement::new(
        neomacs_display_protocol::DisplayFrameId::new(frame_id),
        frame.presentation_id,
        (parent_id != 0).then(|| neomacs_display_protocol::DisplayFrameId::new(parent_id)),
        neomacs_display_protocol::ParentFrameRect::new(x, y, frame.width, frame.height).unwrap(),
        z_order,
    );
}

#[test]
fn wheel_input_atomically_carries_its_presented_region() {
    let (mut app, emacs) = make_test_app_with_input(200, 100, 1.0);
    let window_id = winit::window::WindowId::dummy();
    app.frame_windows.primary_winit_id = Some(window_id);

    let window = app.frame_windows.primary_window_mut().unwrap();
    window.render.set_emacs_frame_id(0x42);
    window.render.set_mouse_pos((50.0, 40.0));
    let mut frame = FrameGlyphBuffer::with_size(200.0, 100.0);
    frame.presentation_id = PresentationId::new(91);
    frame
        .install_presented_hit_index(
            PresentedHitIndex::from_parts(
                frame.presentation_id,
                vec![PresentedHitRegion::new(
                    Some(neomacs_display_protocol::DisplayWindowId::new(1)),
                    PresentedRegionKind::TextBody,
                    FrameRect::new(0.0, 0.0, 200.0, 100.0).unwrap(),
                    0,
                )],
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    window.render.set_current_frame(Some(frame), None);

    app.handle_mouse_wheel(
        window_id,
        winit::event::MouseScrollDelta::LineDelta(0.0, -1.0),
    );

    assert!(matches!(
        emacs.input_rx.try_recv().unwrap(),
        crate::thread_comm::InputEvent::PositionedPointer(
            crate::thread_comm::PositionedPointerInput {
                position: crate::thread_comm::PointerPosition {
                    x: 50.0,
                    y: 40.0,
                    target_frame_id: 0x42,
                },
                target: crate::thread_comm::PointerTarget::Presented {
                    presentation: 91,
                    hit: Some(hit),
                },
                action: crate::thread_comm::PointerAction::Scroll {
                    delta: crate::thread_comm::ScrollDelta::Lines { x: 0.0, y: -1.0 },
                    ..
                },
            }
        ) if hit.region().window()
            == Some(neomacs_display_protocol::DisplayWindowId::new(1))
    ));
    assert!(emacs.input_rx.try_recv().is_err());
}

fn presented_pointer_integration_relief(pressed: bool) -> PointerImageRelief {
    let light = Color::new(0.85, 0.85, 0.85, 1.0);
    let dark = Color::new(0.25, 0.25, 0.25, 1.0);
    let (top_left, bottom_right) = if pressed {
        (dark, light)
    } else {
        (light, dark)
    };
    PointerImageRelief::new(
        top_left,
        bottom_right,
        1.0,
        PointerReliefMargins::new(0.0, 0.0, 0.0, 0.0),
        PointerReliefEdges::new(true, true, true, true),
        PointerReliefCornerErase::new(Color::BLACK, 4.0, 1.0),
    )
}

fn presented_pointer_integration_frame(presentation: u64) -> FrameGlyphBuffer {
    let mut frame = FrameGlyphBuffer::with_size(140.0, 24.0);
    frame.presentation_id = PresentationId::new(presentation);
    frame.background = Color::rgb(0.05, 0.06, 0.07);
    let mut base = Face::new(FaceId::new(0));
    base.background = Color::rgb(0.1, 0.2, 0.8);
    frame.faces.insert(FaceId::new(0), base);
    let mut hover = Face::new(FaceId::new(9));
    hover.background = Color::rgb(0.8, 0.2, 0.1);
    frame.faces.insert(FaceId::new(9), hover);
    frame.add_char('T', 0.0, 0.0, 96.0, 24.0, 18.0, false);
    frame.add_image(ImageId::new(1), 104.0, 4.0, 16.0, 16.0);
    frame
        .install_presented_pointer(
            vec![
                PresentedPointerRegion::new_owned(
                    PresentedRegionId::new(None, PresentedRegionKind::TabBar),
                    FrameRect::new(0.0, 0.0, 80.0, 24.0).unwrap(),
                    Some(InteractionId::new(1)),
                    Some(PointerAppearanceId::try_from(0usize).unwrap()),
                ),
                PresentedPointerRegion::new_owned(
                    PresentedRegionId::new(None, PresentedRegionKind::TabBar),
                    FrameRect::new(80.0, 0.0, 16.0, 24.0).unwrap(),
                    Some(InteractionId::new(2)),
                    Some(PointerAppearanceId::try_from(0usize).unwrap()),
                ),
                PresentedPointerRegion::new_owned(
                    PresentedRegionId::new(None, PresentedRegionKind::TabBar),
                    FrameRect::new(100.0, 0.0, 24.0, 24.0).unwrap(),
                    Some(InteractionId::new(3)),
                    Some(PointerAppearanceId::try_from(1usize).unwrap()),
                ),
            ],
            vec![
                PresentedPointerAppearance::new(
                    vec![PresentedPaintSpan::new(
                        PresentedPrimitiveKind::Glyph,
                        0,
                        1,
                        FrameRect::new(0.0, 0.0, 96.0, 24.0).unwrap(),
                    )],
                    PointerDrawMode::Face(FaceId::new(9)),
                    PointerDrawMode::Face(FaceId::new(9)),
                ),
                PresentedPointerAppearance::new(
                    vec![PresentedPaintSpan::new(
                        PresentedPrimitiveKind::Image,
                        1,
                        1,
                        FrameRect::new(104.0, 4.0, 16.0, 16.0).unwrap(),
                    )],
                    PointerDrawMode::ImageRelief(presented_pointer_integration_relief(false)),
                    PointerDrawMode::ImageRelief(presented_pointer_integration_relief(true)),
                ),
            ],
        )
        .expect("valid integration pointer map");
    frame
        .install_presented_hit_index(
            PresentedHitIndex::from_parts(
                PresentationId::new(presentation),
                vec![PresentedHitRegion::new(
                    None,
                    PresentedRegionKind::TabBar,
                    FrameRect::new(0.0, 0.0, 140.0, 24.0).unwrap(),
                    i32::MAX,
                )],
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    frame
}

struct PresentedPointerRuntimeRenderHarness {
    renderer: neomacs_renderer_wgpu::WgpuRenderer,
    atlas: neomacs_renderer_wgpu::WgpuGlyphAtlas,
    target: wgpu::Texture,
    view: wgpu::TextureView,
}

fn presented_pointer_runtime_render_harness() -> Option<PresentedPointerRuntimeRenderHarness> {
    const WIDTH: u32 = 140;
    const HEIGHT: u32 = 24;
    let renderer = neomacs_renderer_wgpu::WgpuRenderer::new(None, WIDTH, HEIGHT).ok()?;
    let mut atlas = neomacs_renderer_wgpu::WgpuGlyphAtlas::new(renderer.device());
    let initial_frame = FrameGlyphBuffer::default();
    atlas.set_current_frame_fonts(initial_frame.font_bindings());
    let target = renderer.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("presented-pointer-runtime-render-target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    Some(PresentedPointerRuntimeRenderHarness {
        renderer,
        atlas,
        target,
        view,
    })
}

fn presented_pointer_runtime_render_readback(
    harness: &PresentedPointerRuntimeRenderHarness,
) -> Vec<u8> {
    const WIDTH: u32 = 140;
    const HEIGHT: u32 = 24;
    let unpadded = WIDTH * 4;
    let padded =
        unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = harness
        .renderer
        .device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("presented-pointer-runtime-readback"),
            size: u64::from(padded * HEIGHT),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder = harness
        .renderer
        .device()
        .create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &harness.target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    harness
        .renderer
        .queue()
        .submit(std::iter::once(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    harness
        .renderer
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(3)),
        })
        .expect("pointer render poll");
    let mapped = slice
        .get_mapped_range()
        .expect("render readback buffer should remain mapped");
    let mut pixels = vec![0; (unpadded * HEIGHT) as usize];
    for row in 0..HEIGHT {
        let source = (row * padded) as usize;
        let destination = (row * unpadded) as usize;
        pixels[destination..destination + unpadded as usize]
            .copy_from_slice(&mapped[source..source + unpadded as usize]);
    }
    pixels
}

fn presented_pointer_runtime_render_pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    const WIDTH: u32 = 140;
    let index = ((y * WIDTH + x) * 4) as usize;
    [
        pixels[index + 2],
        pixels[index + 1],
        pixels[index],
        pixels[index + 3],
    ]
}

#[test]
fn presented_pointer_integration_runtime_motion_drives_same_frame_mouse_face_pixels() {
    let Some(mut harness) = presented_pointer_runtime_render_harness() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(Some(presented_pointer_integration_frame(69)), None);
    let frame = render.current_frame_clone().unwrap();

    let base_selection = render.pointer_selection_for(&frame);
    harness.renderer.render_frame_glyphs(
        &harness.view,
        &frame,
        &mut harness.atlas,
        test_present_mapping(&frame, 140, 24),
        false,
        None,
        (84.0, 10.0),
        None,
        base_selection,
        None,
    );
    let base = presented_pointer_runtime_render_readback(&harness);

    assert!(render.update_presented_pointer_motion(Some((0x42, 84.0, 10.0))));
    let hover_selection = render.pointer_selection_for(&frame);
    harness.renderer.render_frame_glyphs(
        &harness.view,
        &frame,
        &mut harness.atlas,
        test_present_mapping(&frame, 140, 24),
        false,
        None,
        (84.0, 10.0),
        None,
        hover_selection,
        None,
    );
    let hovered = presented_pointer_runtime_render_readback(&harness);
    assert_eq!(
        render
            .compositor
            .current_frame
            .as_ref()
            .unwrap()
            .presentation_id,
        PresentationId::new(69),
        "motion reuses the displayed presentation"
    );
    let base_pixel = presented_pointer_runtime_render_pixel(&base, 40, 10);
    let hovered_pixel = presented_pointer_runtime_render_pixel(&hovered, 40, 10);
    assert!(
        base_pixel[2] > base_pixel[0] && hovered_pixel[0] > hovered_pixel[2],
        "runtime-selected mouse-face should change blue to red: {base_pixel:?} -> {hovered_pixel:?}"
    );

    assert!(render.update_presented_pointer_motion(None));
    let leave_selection = render.pointer_selection_for(&frame);
    harness.renderer.render_frame_glyphs(
        &harness.view,
        &frame,
        &mut harness.atlas,
        test_present_mapping(&frame, 140, 24),
        false,
        None,
        (130.0, 20.0),
        None,
        leave_selection,
        None,
    );
    let restored = presented_pointer_runtime_render_readback(&harness);
    assert_eq!(restored, base, "runtime leave restores byte-identical base");
}

#[test]
fn presented_pointer_integration_published_frame_drives_hover_press_leave_and_staleness() {
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(Some(presented_pointer_integration_frame(70)), None);
    render.set_dirty(false);

    assert!(render.update_presented_pointer_motion(Some((0x42, 84.0, 10.0))));
    let frame = render.compositor.current_frame.as_ref().unwrap();
    let close = render.pointer_appearance.active().unwrap();
    let close_selection = close.selection_for(frame).expect("current selection");
    assert_eq!(close_selection.appearance().get(), 0);
    assert_eq!(
        frame
            .presented_pointer()
            .appearance(close_selection.appearance())
            .unwrap()
            .hover(),
        PointerDrawMode::Face(FaceId::new(9)),
        "the close hit activates the whole-tab face"
    );
    assert!(render.compositor.dirty);

    render.set_dirty(false);
    assert!(
        !render.update_presented_pointer_motion(Some((0x42, 20.0, 10.0))),
        "moving from close to body stays in the same whole-tab appearance"
    );
    assert!(!render.compositor.dirty);

    assert!(render.update_presented_pointer_motion(Some((0x42, 110.0, 10.0))));
    {
        let frame = render.compositor.current_frame.as_ref().unwrap();
        let plus_hover = render
            .pointer_appearance
            .active()
            .unwrap()
            .selection_for(frame)
            .unwrap();
        let plus = frame
            .presented_pointer()
            .appearance(plus_hover.appearance())
            .unwrap();
        assert_eq!(plus_hover.phase(), ProtocolPointerAppearancePhase::Hover);
        assert_eq!(
            plus.hover(),
            PointerDrawMode::ImageRelief(presented_pointer_integration_relief(false))
        );
    }

    assert!(render.update_presented_pointer_button(Some((0x42, 110.0, 10.0)), true));
    let frame = render.compositor.current_frame.as_ref().unwrap();
    let plus_pressed = render
        .pointer_appearance
        .active()
        .unwrap()
        .selection_for(frame)
        .unwrap();
    let plus = frame
        .presented_pointer()
        .appearance(plus_pressed.appearance())
        .unwrap();
    assert_eq!(
        plus_pressed.phase(),
        ProtocolPointerAppearancePhase::Pressed
    );
    assert_eq!(
        plus.pressed(),
        PointerDrawMode::ImageRelief(presented_pointer_integration_relief(true))
    );
    assert!(render.update_presented_pointer_button(Some((0x42, 110.0, 10.0)), false));

    assert!(render.update_presented_pointer_motion(None));
    assert_eq!(render.pointer_appearance.active(), None);
    assert!(render.update_presented_pointer_motion(Some((0x42, 84.0, 10.0))));
    assert!(render.pointer_appearance.active().is_some());
    render.set_dirty(false);
    render.set_current_frame(Some(presented_pointer_integration_frame(71)), None);
    assert_eq!(render.pointer_appearance.active(), None);
    assert!(
        render.compositor.dirty,
        "retiring active paint dirties base rendering"
    );
    assert_eq!(
        render.pointer_selection_for(render.compositor.current_frame.as_ref().unwrap()),
        None
    );
}

#[test]
fn presented_pointer_integration_damage_unions_old_and_new_paint_spans() {
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(Some(presented_pointer_integration_frame(75)), None);

    assert!(render.update_presented_pointer_motion(Some((0x42, 84.0, 10.0))));
    assert_eq!(
        render.pointer_paint_damage(),
        [Some(FrameRect::new(0.0, 0.0, 96.0, 24.0).unwrap()), None]
    );
    render.finish_pointer_paint_render();
    assert!(render.update_presented_pointer_button(Some((0x42, 84.0, 10.0)), true));
    assert_eq!(
        render.pointer_paint_damage(),
        [Some(FrameRect::new(0.0, 0.0, 96.0, 24.0).unwrap()), None]
    );
    render.finish_pointer_paint_render();
    assert!(render.update_presented_pointer_button(Some((0x42, 84.0, 10.0)), false));
    assert_eq!(
        render.pointer_paint_damage(),
        [Some(FrameRect::new(0.0, 0.0, 96.0, 24.0).unwrap()), None]
    );
    render.finish_pointer_paint_render();
    assert!(!render.update_presented_pointer_motion(Some((0x42, 20.0, 10.0))));
    assert_eq!(render.pointer_paint_damage(), [None, None]);

    assert!(render.update_presented_pointer_motion(Some((0x42, 110.0, 10.0))));
    assert_eq!(
        render.pointer_paint_damage(),
        [
            Some(FrameRect::new(0.0, 0.0, 96.0, 24.0).unwrap()),
            Some(FrameRect::new(104.0, 4.0, 16.0, 16.0).unwrap()),
        ]
    );
    render.finish_pointer_paint_render();
    assert!(render.update_presented_pointer_motion(None));
    assert_eq!(
        render.pointer_paint_damage(),
        [Some(FrameRect::new(104.0, 4.0, 16.0, 16.0).unwrap()), None]
    );

    render.finish_pointer_paint_render();
    assert!(render.update_presented_pointer_motion(Some((0x42, 84.0, 10.0))));
    render.finish_pointer_paint_render();
    render.set_current_frame(Some(presented_pointer_integration_frame(76)), None);
    assert_eq!(
        render.pointer_paint_damage(),
        [Some(FrameRect::new(0.0, 0.0, 96.0, 24.0).unwrap()), None],
        "new presentation invalidates the old active paint only"
    );
}

#[test]
fn unchanged_pointer_appearance_skips_damage_inspection_in_a_ten_thousand_glyph_frame() {
    let mut frame = FrameGlyphBuffer::with_size(10_000.0, 20.0);
    frame.presentation_id = PresentationId::new(77);
    frame
        .faces
        .insert(FaceId::new(9), Face::new(FaceId::new(9)));
    for x in 0..10_000 {
        frame.add_char('x', x as f32, 0.0, 1.0, 10.0, 8.0, false);
    }
    frame
        .install_presented_pointer(
            vec![PresentedPointerRegion::new_owned(
                PresentedRegionId::new(None, PresentedRegionKind::TabBar),
                FrameRect::new(9_999.0, 0.0, 1.0, 10.0).unwrap(),
                None,
                Some(PointerAppearanceId::try_from(0usize).unwrap()),
            )],
            vec![PresentedPointerAppearance::new(
                vec![PresentedPaintSpan::new(
                    PresentedPrimitiveKind::Glyph,
                    9_999,
                    1,
                    FrameRect::new(9_999.0, 0.0, 1.0, 10.0).unwrap(),
                )],
                PointerDrawMode::Face(FaceId::new(9)),
                PointerDrawMode::Face(FaceId::new(9)),
            )],
        )
        .unwrap();
    frame
        .install_presented_hit_index(
            PresentedHitIndex::from_parts(
                frame.presentation_id,
                vec![PresentedHitRegion::new(
                    None,
                    PresentedRegionKind::TabBar,
                    FrameRect::new(0.0, 0.0, 10_000.0, 20.0).unwrap(),
                    0,
                )],
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(Some(frame), None);
    assert!(render.update_presented_pointer_motion(Some((0x42, 9_999.5, 5.0))));
    render.finish_pointer_paint_render();
    let lookups = render.pointer_damage_appearance_lookups();

    for _ in 0..10_000 {
        assert!(!render.update_presented_pointer_motion(Some((0x42, 9_999.5, 5.0))));
    }

    assert_eq!(render.pointer_damage_appearance_lookups(), lookups);
    assert_eq!(render.pointer_paint_damage(), [None, None]);
}

fn presented_pointer_integration_offset_frame(
    presentation: u64,
    width: f32,
    height: f32,
    regions: &[(f32, f32, f32, f32)],
) -> FrameGlyphBuffer {
    let mut frame = FrameGlyphBuffer::with_size(width, height);
    frame.presentation_id = PresentationId::new(presentation);
    frame
        .faces
        .insert(FaceId::new(0), Face::new(FaceId::new(0)));
    frame
        .faces
        .insert(FaceId::new(9), Face::new(FaceId::new(9)));
    let (x, y, glyph_width, glyph_height) = regions[0];
    frame.add_char(
        'x',
        x,
        y,
        glyph_width,
        glyph_height,
        glyph_height - 2.0,
        false,
    );
    frame
        .install_presented_pointer(
            regions
                .iter()
                .map(|&(x, y, width, height)| {
                    PresentedPointerRegion::new_owned(
                        PresentedRegionId::new(None, PresentedRegionKind::TabBar),
                        FrameRect::new(x, y, width, height).unwrap(),
                        Some(InteractionId::new(1)),
                        Some(PointerAppearanceId::try_from(0usize).unwrap()),
                    )
                })
                .collect(),
            vec![PresentedPointerAppearance::new(
                vec![PresentedPaintSpan::new(
                    PresentedPrimitiveKind::Glyph,
                    0,
                    1,
                    FrameRect::new(x, y, glyph_width, glyph_height).unwrap(),
                )],
                PointerDrawMode::Face(FaceId::new(9)),
                PointerDrawMode::Face(FaceId::new(9)),
            )],
        )
        .unwrap();
    frame
        .install_presented_hit_index(
            PresentedHitIndex::from_parts(
                PresentationId::new(presentation),
                vec![PresentedHitRegion::new(
                    None,
                    PresentedRegionKind::TabBar,
                    FrameRect::new(0.0, 0.0, width, height).unwrap(),
                    0,
                )],
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    frame
}

#[test]
fn presented_pointer_integration_topmost_child_uses_local_coordinates_then_root_fallback() {
    let mut app = make_test_app(300, 200, 1.0);
    let render = ensure_primary_frame(&mut app).expect("primary render");
    render.set_emacs_frame_id(0x42);
    let mut root = presented_pointer_integration_offset_frame(
        80,
        300.0,
        200.0,
        &[(110.0, 85.0, 20.0, 10.0), (10.0, 5.0, 20.0, 10.0)],
    );
    set_test_frame_placement(&mut root, 0x42, 0, 0.0, 0.0, 0);
    render.set_current_frame(Some(root), None);
    let mut lower_child =
        presented_pointer_integration_offset_frame(82, 60.0, 40.0, &[(10.0, 5.0, 20.0, 10.0)]);
    set_test_frame_placement(&mut lower_child, 0x98, 0x42, 100.0, 80.0, 5);
    render.update_child_frame(lower_child);
    let mut child =
        presented_pointer_integration_offset_frame(81, 60.0, 40.0, &[(10.0, 5.0, 20.0, 10.0)]);
    set_test_frame_placement(&mut child, 0x99, 0x42, 100.0, 80.0, 10);
    render.update_child_frame(child);

    let window = app.frame_windows.primary_window_mut().unwrap();
    let child_owner = RenderApp::pointer_owner(window, 115.0, 88.0);
    assert!(matches!(
        child_owner,
        PointerOwner::Child {
            frame_id: 0x99,
            x: 15.0,
            y: 8.0
        }
    ));
    let child_target = child_owner
        .target()
        .map(|(x, y, frame_id)| (frame_id, x, y));
    assert!(window.render.update_presented_pointer_motion(child_target));
    assert_eq!(
        window.render.pointer_paint_damage(),
        [Some(FrameRect::new(110.0, 85.0, 20.0, 10.0).unwrap()), None],
        "child paint clips are translated into parent surface coordinates"
    );
    window.render.finish_pointer_paint_render();
    assert_eq!(
        window
            .render
            .pointer_appearance
            .active()
            .unwrap()
            .presentation(),
        PresentationId::new(81)
    );

    let child_press = RenderApp::capture_presented_pointer_press(window, child_owner, 115.0, 88.0)
        .unwrap()
        .expect("topmost child interaction");
    assert!(matches!(
        child_press,
        crate::thread_comm::InputEvent::PresentedPointer {
            presentation: 81,
            interaction: 1,
            pressed: true,
            emacs_frame_id: 0x99,
            x: 15.0,
            y: 8.0,
            ..
        }
    ));
    let root_motion = PointerOwner::Root {
        frame_id: 0x42,
        x: 15.0,
        y: 8.0,
    };
    let root_motion_target = root_motion
        .target()
        .map(|(x, y, frame_id)| (frame_id, x, y));
    window
        .render
        .update_presented_pointer_motion(root_motion_target);
    assert_eq!(
        window
            .render
            .presented_capture()
            .and_then(|capture| capture.target())
            .map(|target| target.presentation()),
        Some(PresentationId::new(81)),
        "motion onto the root must not retarget the child press"
    );
    assert_eq!(window.render.route_presentation_retirement(81), None);
    window.render.clear_pointer_hover();
    let mut replacement = presented_pointer_integration_frame(83);
    set_test_frame_placement(&mut replacement, 0x99, 0x42, 140.0, 120.0, 10);
    window.render.update_child_frame(replacement);
    let child_release = RenderApp::take_presented_release_events(&mut window.render, 200.0, 160.0);
    // Capture pins the press snapshot transform: replacement movement cannot
    // reinterpret the release coordinates for the evaluator-owned gesture.
    assert!(matches!(
        child_release.as_slice(),
        [
            crate::thread_comm::InputEvent::PresentedPointer {
                presentation: 81,
                interaction: 1,
                pressed: false,
                emacs_frame_id: 0x99,
                x: 100.0,
                y: 80.0,
                ..
            },
            crate::thread_comm::InputEvent::PresentationRetired { presentation: 81 }
        ]
    ));

    let root_owner = RenderApp::pointer_owner(window, 15.0, 8.0);
    assert!(matches!(
        root_owner,
        PointerOwner::Root { frame_id: 0x42, .. }
    ));
    let root_target = root_owner.target().map(|(x, y, frame_id)| (frame_id, x, y));
    assert!(window.render.update_presented_pointer_motion(root_target));
    assert_eq!(
        window
            .render
            .pointer_appearance
            .active()
            .unwrap()
            .presentation(),
        PresentationId::new(80)
    );
    let root_press = RenderApp::capture_presented_pointer_press(window, root_owner, 15.0, 8.0)
        .unwrap()
        .expect("root interaction");
    assert!(matches!(
        root_press,
        crate::thread_comm::InputEvent::PresentedPointer {
            emacs_frame_id: 0x42,
            x: 15.0,
            y: 8.0,
            ..
        }
    ));
}

#[test]
fn real_layout_publication_routes_overlapping_children_by_published_z_order() {
    let mut eval = neovm_core::emacs_core::Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("child");
    let root = eval
        .frame_manager_mut()
        .create_frame("root-hit", 200, 120, buffer);
    let lower = eval
        .frame_manager_mut()
        .create_frame("lower-hit", 80, 60, buffer);
    let upper = eval
        .frame_manager_mut()
        .create_frame("upper-hit", 80, 60, buffer);
    for (frame_id, parent, z_order) in [
        (root, None, 0),
        (lower, Some(root), 2),
        (upper, Some(root), 9),
    ] {
        let frame = eval.frame_manager_mut().get_mut(frame_id).unwrap();
        frame.set_window_system(Some(neovm_core::emacs_core::Value::symbol("neo")));
        frame.z_order = z_order;
        if let Some(parent) = parent {
            frame.parent_frame = neovm_core::emacs_core::Value::make_frame(parent.0);
            frame.left_pos = 20;
            frame.top_pos = 15;
        }
    }
    let mut engine = neomacs_layout_engine::LayoutEngine::new();
    let mut layout = |frame_id| match engine.redisplay_frame_attempt(&mut eval, frame_id) {
        neomacs_layout_engine::engine::FrameLayoutAttempt::Prepared(frame) => frame.materialize(),
        neomacs_layout_engine::engine::FrameLayoutAttempt::Aborted => {
            panic!("hit-test fixture layout aborted")
        }
    };
    let root_frame = layout(root);
    let lower_frame = layout(lower);
    let upper_frame = layout(upper);

    let mut app = make_test_app(200, 120, 1.0);
    let render = ensure_primary_frame(&mut app).expect("primary render");
    render.set_emacs_frame_id(root.0);
    render.set_current_frame(Some(root_frame), None);
    assert!(render.update_child_frame(lower_frame));
    assert!(render.update_child_frame(upper_frame));
    let window = app.frame_windows.primary_window().unwrap();
    let owner = RenderApp::pointer_owner(window, 25.0, 20.0);
    assert!(matches!(
        owner,
        PointerOwner::Child { frame_id, .. } if frame_id == upper.0
    ));
    let (x, y, frame_id) = owner.target().unwrap();
    let (_, semantic) = window
        .render
        .presented_region_observation(frame_id, x, y)
        .unwrap()
        .unwrap();
    assert!(
        semantic.is_some(),
        "topmost real child must own semantic hit"
    );
}

#[test]
fn real_layout_publication_keeps_nested_child_parent_relative_and_runtime_places_it_in_root() {
    let mut eval = neovm_core::emacs_core::Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let root = eval
        .frame_manager_mut()
        .create_frame("root-nested", 800, 600, buffer);
    let parent = eval
        .frame_manager_mut()
        .create_frame("parent-nested", 300, 200, buffer);
    let nested = eval
        .frame_manager_mut()
        .create_frame("nested", 100, 80, buffer);
    for frame_id in [root, parent, nested] {
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .unwrap()
            .set_window_system(Some(neovm_core::emacs_core::Value::symbol("neo")));
    }
    {
        let frame = eval.frame_manager_mut().get_mut(parent).unwrap();
        frame.parent_frame = neovm_core::emacs_core::Value::make_frame(root.0);
        frame.left_pos = 100;
        frame.top_pos = 80;
    }
    {
        let frame = eval.frame_manager_mut().get_mut(nested).unwrap();
        frame.parent_frame = neovm_core::emacs_core::Value::make_frame(parent.0);
        frame.left_pos = 15;
        frame.top_pos = 12;
    }

    let mut engine = neomacs_layout_engine::LayoutEngine::new_without_font_metrics();
    let mut layout = |frame_id| match engine.redisplay_frame_attempt(&mut eval, frame_id) {
        neomacs_layout_engine::engine::FrameLayoutAttempt::Prepared(frame) => frame.materialize(),
        neomacs_layout_engine::engine::FrameLayoutAttempt::Aborted => {
            panic!("nested-frame fixture layout aborted")
        }
    };
    let root_frame = layout(root);
    let parent_frame = layout(parent);
    let nested_frame = layout(nested);
    assert_eq!(
        (
            nested_frame.frame_placement.outer_in_parent().x(),
            nested_frame.frame_placement.outer_in_parent().y(),
        ),
        (15.0, 12.0),
        "layout transport must preserve the nested frame's immediate-parent coordinates"
    );

    let mut app = make_test_app(800, 600, 1.0);
    let render = ensure_primary_frame(&mut app).expect("primary render");
    render.set_emacs_frame_id(root.0);
    render.set_current_frame(Some(root_frame), None);
    assert!(render.update_child_frame(parent_frame));
    assert!(render.update_child_frame(nested_frame));
    let nested_entry = render
        .compositor
        .child_frames
        .frames
        .get(&nested.0)
        .expect("nested runtime frame");
    assert_eq!((nested_entry.abs_x, nested_entry.abs_y), (115.0, 92.0));
}

#[test]
fn presented_pointer_integration_close_and_add_dispatch_distinct_interactions() {
    let mut app = make_test_app(140, 24, 1.0);
    let Some(render) = ensure_primary_frame(&mut app) else {
        return;
    };
    render.set_emacs_frame_id(0x42);
    render.set_current_frame(Some(presented_pointer_integration_frame(90)), None);
    let window = app.frame_windows.primary_window_mut().unwrap();

    let semantic = window
        .render
        .presented_region_hit(0x42, PresentationId::new(90), 84.0, 10.0)
        .unwrap()
        .unwrap();
    let appearance = window
        .render
        .presented_pointer_hit(0x42, 84.0, 10.0)
        .unwrap()
        .unwrap();
    assert_eq!(semantic.region().kind(), PresentedRegionKind::TabBar);
    assert_eq!(appearance.presentation(), PresentationId::new(90));
    assert_eq!(appearance.interaction(), Some(InteractionId::new(2)));

    let body = RenderApp::capture_tab_band_press(window, 20.0, 10.0).unwrap();
    let body_release = RenderApp::take_presented_release_events(&mut window.render, 20.0, 10.0);
    let close = RenderApp::capture_tab_band_press(window, 84.0, 10.0).unwrap();
    let close_release = RenderApp::take_presented_release_events(&mut window.render, 84.0, 10.0);
    let add = RenderApp::capture_tab_band_press(window, 110.0, 10.0).unwrap();
    let add_release = RenderApp::take_presented_release_events(&mut window.render, 110.0, 10.0);

    assert!(matches!(
        body,
        crate::thread_comm::InputEvent::PresentedPointer {
            presentation: 90,
            interaction: 1,
            pressed: true,
            ..
        }
    ));
    assert!(matches!(
        body_release.as_slice(),
        [crate::thread_comm::InputEvent::PresentedPointer {
            presentation: 90,
            interaction: 1,
            pressed: false,
            ..
        }]
    ));
    assert!(matches!(
        close,
        crate::thread_comm::InputEvent::PresentedPointer {
            presentation: 90,
            interaction: 2,
            pressed: true,
            ..
        }
    ));
    assert!(matches!(
        close_release.as_slice(),
        [crate::thread_comm::InputEvent::PresentedPointer {
            presentation: 90,
            interaction: 2,
            pressed: false,
            ..
        }]
    ));
    assert!(matches!(
        add,
        crate::thread_comm::InputEvent::PresentedPointer {
            presentation: 90,
            interaction: 3,
            pressed: true,
            ..
        }
    ));
    assert!(matches!(
        add_release.as_slice(),
        [crate::thread_comm::InputEvent::PresentedPointer {
            presentation: 90,
            interaction: 3,
            pressed: false,
            ..
        }]
    ));
}

#[test]
fn active_pointer_appearance_selects_only_its_exact_presented_frame() {
    let mut frame = FrameGlyphBuffer::with_size(100.0, 20.0);
    frame.presentation_id = PresentationId::new(7);
    frame
        .faces
        .insert(FaceId::new(9), crate::core::face::Face::new(FaceId::new(9)));
    frame.add_char('a', 0.0, 0.0, 10.0, 20.0, 15.0, false);
    frame
        .install_presented_pointer(
            vec![PresentedPointerRegion::new(
                FrameRect::new(0.0, 0.0, 10.0, 20.0).unwrap(),
                None,
                Some(PointerAppearanceId::try_from(0usize).unwrap()),
            )],
            vec![PresentedPointerAppearance::new(
                vec![PresentedPaintSpan::new(
                    PresentedPrimitiveKind::Glyph,
                    0,
                    1,
                    FrameRect::new(0.0, 0.0, 10.0, 20.0).unwrap(),
                )],
                PointerDrawMode::Face(FaceId::new(9)),
                PointerDrawMode::Face(FaceId::new(9)),
            )],
        )
        .unwrap();
    let active =
        ActivePointerAppearance::new(appearance_key(7, 0), PointerAppearancePhase::Pressed);

    let selection = active.selection_for(&frame).expect("matching snapshot");
    assert_eq!(
        selection.appearance(),
        PointerAppearanceId::try_from(0usize).unwrap()
    );
    assert_eq!(selection.phase(), ProtocolPointerAppearancePhase::Pressed);

    frame.presentation_id = PresentationId::new(8);
    assert_eq!(active.selection_for(&frame), None);
}

#[test]
fn pointer_appearance_is_qualified_by_presentation_and_phase() {
    let first = appearance_key(7, 1);
    let second = appearance_key(7, 2);
    let replacement = appearance_key(8, 1);
    let mut state = PointerAppearanceState::default();

    assert!(state.hover(Some(first)));
    assert_eq!(
        state.active(),
        Some(ActivePointerAppearance::new(
            first,
            PointerAppearancePhase::Hover
        ))
    );
    assert!(!state.hover(Some(first)), "same visual range is stable");

    assert!(state.press());
    assert_eq!(
        state.active().unwrap().phase(),
        PointerAppearancePhase::Pressed
    );
    assert!(
        !state.press(),
        "repeated press does not change the draw phase"
    );

    assert!(state.hover(Some(second)));
    assert_eq!(
        state.active(),
        Some(ActivePointerAppearance::new(
            second,
            PointerAppearancePhase::Hover
        ))
    );
    assert!(state.hover(Some(first)));
    assert_eq!(
        state.active().unwrap().phase(),
        PointerAppearancePhase::Pressed
    );

    assert!(state.release());
    assert_eq!(
        state.active().unwrap().phase(),
        PointerAppearancePhase::Hover
    );
    assert!(!state.release(), "repeated release is visually stable");
    assert!(state.hover(Some(replacement)));
    assert_eq!(
        state.active().unwrap().presentation(),
        PresentationId::new(8)
    );
    assert!(state.hover(None));
    assert_eq!(state.active(), None);
}

#[test]
fn pressed_visual_stays_captured_while_hover_follows_pointer() {
    let pressed_visual = appearance_key(11, 3);
    let other_visual = appearance_key(11, 4);
    let mut state = PointerAppearanceState::default();

    state.hover(Some(pressed_visual));
    state.press();
    state.hover(Some(other_visual));

    assert_eq!(state.pressed(), Some(pressed_visual));
    assert_eq!(state.active().unwrap().key(), other_visual);
    assert_eq!(
        state.active().unwrap().phase(),
        PointerAppearancePhase::Hover
    );
    assert!(
        state.release(),
        "clearing the captured visual is a state change even while another visual is active"
    );
    assert_eq!(state.active().unwrap().key(), other_visual);
    assert_eq!(
        state.active().unwrap().phase(),
        PointerAppearancePhase::Hover
    );
    assert_eq!(state.pressed(), None);

    state.hover(Some(pressed_visual));
    state.press();
    state.hover(Some(appearance_key(12, 4)));
    assert!(
        state.retire(PresentationId::new(11)),
        "retiring a captured visual is a state change even if another presentation is hovered"
    );
    assert_eq!(
        state.active().unwrap().presentation(),
        PresentationId::new(12)
    );
    assert_eq!(state.pressed(), None);

    state.hover(Some(pressed_visual));
    assert_eq!(
        state.active().unwrap().phase(),
        PointerAppearancePhase::Hover
    );
}

#[test]
fn visual_transitions_do_not_mutate_evaluator_press_capture() {
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let target = PresentedInteractionKey::new(PresentationId::new(11), InteractionId::new(99));
    render.capture_presented(Some(target));

    render.pointer_appearance.hover(Some(appearance_key(11, 3)));
    render.pointer_appearance.press();
    render.pointer_appearance.hover(Some(appearance_key(11, 4)));
    render.pointer_appearance.release();

    assert_eq!(
        render.presented_capture(),
        Some(PresentedPressCapture::new(Some(target)))
    );
}

#[test]
fn cursor_leave_clears_hover_but_preserves_visual_and_input_capture() {
    let pressed_visual = appearance_key(11, 3);
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let target = PresentedInteractionKey::new(PresentationId::new(11), InteractionId::new(99));
    render.capture_presented(Some(target));
    render.pointer_appearance.hover(Some(pressed_visual));
    render.pointer_appearance.press();
    render.set_dirty(false);

    assert!(render.clear_pointer_hover());

    assert_eq!(render.pointer_appearance.active(), None);
    assert_eq!(render.pointer_appearance.pressed(), Some(pressed_visual));
    assert_eq!(
        render.presented_capture(),
        Some(PresentedPressCapture::new(Some(target)))
    );
    assert!(render.compositor.dirty);
}

#[test]
fn tab_release_uses_the_original_capture_after_hover_moves_or_leaves() {
    let captured = PresentedInteractionKey::new(PresentationId::new(21), InteractionId::new(7));
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.capture_presented(Some(captured));
    render.pointer_appearance.hover(Some(appearance_key(21, 1)));
    render.pointer_appearance.press();
    render.pointer_appearance.hover(Some(appearance_key(21, 2)));

    let event = RenderApp::take_presented_release_events(&mut render, 500.0, 700.0)
        .into_iter()
        .next()
        .expect("captured release event");

    assert!(matches!(
        event,
        crate::thread_comm::InputEvent::PresentedPointer {
            presentation: 21,
            interaction: 7,
            pressed: false,
            x: 500.0,
            y: 700.0,
            ..
        }
    ));
    assert_eq!(render.presented_capture(), None);
}

#[test]
fn blank_tab_band_capture_suppresses_release_without_fake_interaction() {
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.capture_presented(None);

    assert_eq!(
        render.presented_capture(),
        Some(PresentedPressCapture::new(None))
    );
    assert!(RenderApp::take_presented_release_events(&mut render, 40.0, 10.0).is_empty());
    assert_eq!(render.presented_capture(), None);
}

#[test]
fn captured_release_precedes_deferred_presentation_retirement() {
    let target = PresentedInteractionKey::new(PresentationId::new(41), InteractionId::new(9));
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.capture_presented(Some(target));
    assert_eq!(render.route_presentation_retirement(41), None);

    let events = RenderApp::take_presented_release_events(&mut render, 1.0, 2.0);
    assert!(matches!(
        events.as_slice(),
        [
            crate::thread_comm::InputEvent::PresentedPointer {
                presentation: 41,
                interaction: 9,
                pressed: false,
                ..
            },
            crate::thread_comm::InputEvent::PresentationRetired { presentation: 41 }
        ]
    ));
}

#[test]
fn cancellation_flushes_pinned_retirement_without_release() {
    let target = PresentedInteractionKey::new(PresentationId::new(51), InteractionId::new(10));
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.capture_presented(Some(target));
    render.chrome.interaction.toolbar_press_captured = true;
    render.chrome.interaction.toolbar_pressed = Some(3);
    render.pointer_appearance.hover(Some(appearance_key(51, 1)));
    render.pointer_appearance.press();
    assert_eq!(render.route_presentation_retirement(51), None);

    let (changed, retirements) = render.cancel_pointer_interaction();
    assert!(changed);
    assert_eq!(retirements, vec![51]);
    assert_eq!(render.pointer_appearance.active(), None);
    assert_eq!(render.pointer_appearance.pressed(), None);
    assert_eq!(render.presented_capture(), None);
    assert!(!render.chrome.interaction.toolbar_press_captured);
    assert_eq!(render.chrome.interaction.toolbar_pressed, None);
    assert!(!render.pointer_inside);
}

#[test]
fn focus_cancellation_dirties_toolbar_and_compact_only_state() {
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.pointer_inside = true;
    render.chrome.interaction.toolbar_hovered = Some(1);
    render.chrome.interaction.toolbar_pressed = Some(1);
    render.chrome.interaction.toolbar_press_captured = true;
    render.chrome.interaction.compact_bar_tool_hovered = Some(2);
    render.chrome.interaction.compact_bar_tool_pressed = Some(2);
    render.set_dirty(false);

    let (changed, retirements) = render.cancel_pointer_interaction();

    assert!(changed);
    assert!(retirements.is_empty());
    assert!(render.compositor.dirty);
    assert!(!render.pointer_inside);
    assert_eq!(render.chrome.interaction.toolbar_hovered, None);
    assert_eq!(render.chrome.interaction.toolbar_pressed, None);
    assert_eq!(render.chrome.interaction.compact_bar_tool_hovered, None);
    assert_eq!(render.chrome.interaction.compact_bar_tool_pressed, None);
}

#[test]
fn programmatic_popup_open_suppresses_underlying_hover_immediately() {
    let key = appearance_key(61, 1);
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.pointer_appearance.hover(Some(key));
    render.pointer_appearance.press();
    render.set_dirty(false);

    render.set_popup_menu(Some(neomacs_renderer_wgpu::PopupMenuState::new(
        0.0,
        0.0,
        vec![],
        None,
        13.0,
        17.0,
        8.0,
    )));

    assert_eq!(render.pointer_appearance.active(), None);
    assert_eq!(render.pointer_appearance.pressed(), Some(key));
    assert!(render.compositor.dirty);
}

#[test]
fn non_root_owner_clears_stale_root_chrome_hover() {
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.chrome.interaction.menu_bar_hovered = Some(1);
    render.chrome.interaction.compact_bar_menu_hovered = Some(2);
    render.chrome.interaction.compact_bar_tool_hovered = Some(3);
    render.chrome.interaction.toolbar_hovered = Some(4);

    assert!(RenderApp::suppress_root_chrome_hover(&mut render));
    assert_eq!(render.chrome.interaction.menu_bar_hovered, None);
    assert_eq!(render.chrome.interaction.compact_bar_menu_hovered, None);
    assert_eq!(render.chrome.interaction.compact_bar_tool_hovered, None);
    assert_eq!(render.chrome.interaction.toolbar_hovered, None);
}

#[test]
fn tab_capture_survives_tab_bar_removal_until_release() {
    let captured = PresentedInteractionKey::new(PresentationId::new(31), InteractionId::new(8));
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.capture_presented(Some(captured));

    render.chrome.interaction.clear_tab_bar();

    assert_eq!(
        render.presented_capture(),
        Some(PresentedPressCapture::new(Some(captured)))
    );
}

#[test]
fn topmost_child_blocks_root_chrome_ownership() {
    let mut app = make_test_app(800, 600, 1.0);
    let render = ensure_primary_frame(&mut app).expect("primary render");
    render.set_emacs_frame_id(0x42);
    let mut root = FrameGlyphBuffer::with_size(800.0, 600.0);
    set_test_frame_placement(&mut root, 0x42, 0, 0.0, 0.0, 0);
    render.set_current_frame(Some(root), None);
    let mut child = FrameGlyphBuffer::with_size(100.0, 100.0);
    set_test_frame_placement(&mut child, 0x99, 0x42, 0.0, 40.0, 0);
    render.update_child_frame(child);
    let window = app.frame_windows.primary_window().unwrap();

    assert!(matches!(
        RenderApp::pointer_owner(window, 20.0, 56.0),
        PointerOwner::Child { frame_id: 0x99, .. }
    ));
}

#[test]
fn nested_child_ime_cursor_area_uses_presented_root_relative_placement() {
    let mut app = make_test_app(800, 600, 1.0);
    let render = ensure_primary_frame(&mut app).expect("primary render");
    render.set_emacs_frame_id(0x42);

    let mut root = FrameGlyphBuffer::with_size(800.0, 600.0);
    set_test_frame_placement(&mut root, 0x42, 0, 0.0, 0.0, 0);
    render.set_current_frame(Some(root), None);

    let mut parent = FrameGlyphBuffer::with_size(300.0, 200.0);
    set_test_frame_placement(&mut parent, 0x50, 0x42, 100.0, 80.0, 1);
    assert!(render.update_child_frame(parent));

    let mut nested = FrameGlyphBuffer::with_size(120.0, 80.0);
    set_test_frame_placement(&mut nested, 0x51, 0x50, 15.0, 12.0, 2);
    assert!(render.update_child_frame(nested));

    let area = app.ime_cursor_area_for_target(&crate::render_thread::cursor::CursorTarget {
        window_id: 9,
        x: 7.0,
        y: 9.0,
        width: 8.0,
        height: 16.0,
        style: crate::core::frame_glyphs::CursorStyle::FilledBox,
        frame_id: 0x51,
    });

    assert_eq!(
        area,
        crate::render_thread::state::ImeCursorArea {
            x: 122,
            y: 117,
            width: 8,
            height: 16,
        }
    );
}

#[test]
fn popup_owns_pointer_above_underlying_presented_content() {
    let mut app = make_test_app(800, 600, 1.0);
    let render = ensure_primary_frame(&mut app).expect("primary render");
    render.set_popup_menu(Some(neomacs_renderer_wgpu::PopupMenuState::new(
        0.0,
        0.0,
        vec![],
        None,
        13.0,
        17.0,
        8.0,
    )));
    let window = app.frame_windows.primary_window().unwrap();

    let owner = RenderApp::pointer_owner(window, 20.0, 56.0);
    assert_eq!(owner, PointerOwner::Popup);
    assert!(
        owner.target().is_none(),
        "popup suppresses underlying appearance"
    );
    assert!(
        owner.permits_root_chrome(),
        "popup branch retains explicit chrome delegation"
    );
}

fn ensure_primary_frame(app: &mut RenderApp) -> Option<&mut GuiFrameRenderState> {
    if app
        .frame_windows
        .primary_window()
        .map(|ws| &ws.render)
        .is_none()
    {
        let device = make_test_device()?;
        let __render = GuiFrameRenderState::new(
            0,
            &device,
            app.frame_windows
                .primary_window()
                .map_or(1.0, |ws| ws.scale_factor()),
            false,
            neomacs_display_protocol::frame_time::observe_platform_now(),
        );
        if let Some(window_state) = app.frame_windows.primary_window_mut() {
            window_state.render = __render;
        }
    }
    app.frame_windows
        .primary_window_mut()
        .map(|ws| &mut ws.render)
}

#[test]
fn frame_chrome_toolbar_origin_comes_from_authoritative_band_bounds() {
    use neomacs_display_protocol::frame_chrome::{
        ChromeBandRequest, FrameChrome, FrameChromeContent, FrameChromeKind, FrameSize,
        MenuBarContent, ToolBarContent,
    };

    let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.frame_chrome = FrameChrome::layout(
        FrameSize::new(800.0, 600.0).expect("frame size"),
        vec![
            ChromeBandRequest::new(
                FrameChromeKind::MenuBar,
                19.0,
                FrameChromeContent::MenuBar(MenuBarContent::empty()),
            ),
            ChromeBandRequest::new(
                FrameChromeKind::ToolBar,
                41.0,
                FrameChromeContent::ToolBar(ToolBarContent::empty()),
            ),
        ],
    )
    .expect("frame chrome");

    let bounds = crate::render_thread::render_pass::frame_chrome_toolbar_bounds(&frame)
        .expect("toolbar band");
    assert_eq!(bounds.y(), 19.0);
    assert_eq!(bounds.height(), 41.0);
}

#[test]
fn chrome_hit_uses_absolute_semantic_hit_regions() {
    use neomacs_display_protocol::frame_chrome::{
        BandRect, ChromeAction, ChromeBandRequest, ChromeDisplayRow, ChromeHitRegion, FrameChrome,
        FrameChromeContent, FrameChromeKind, FrameSize, MenuBarContent, PositionedChromeItem,
        ToolBarContent,
    };
    use neomacs_display_protocol::{MenuBarItem, ToolBarItem, ToolBarItemType};

    let menu = MenuBarContent::new(
        vec![PositionedChromeItem::new(
            BandRect::new(8.0, 0.0, 48.0, 18.0).expect("menu bounds"),
            MenuBarItem {
                index: 0,
                label: "File".into(),
                key: "file".into(),
            },
            ChromeAction::OpenMenu {
                index: 0,
                key: "file".into(),
            },
        )],
        Color::WHITE,
        Color::BLACK,
    );
    let tool = ToolBarContent::new(
        vec![PositionedChromeItem::new(
            BandRect::new(5.0, 0.0, 24.0, 34.0).expect("tool bounds"),
            ToolBarItem {
                index: 0,
                key: "open".into(),
                image: None,
                label: String::new(),
                help: String::new(),
                enabled: true,
                selected: false,
                item_type: ToolBarItemType::Button,
                wrap: false,
            },
            ChromeAction::InvokeToolBarItem { index: 0 },
        )],
        Color::WHITE,
        Color::BLACK,
        24,
        5,
    );
    let tab = ChromeBandRequest::new(
        FrameChromeKind::TabBar,
        18.0,
        FrameChromeContent::DisplayRow(ChromeDisplayRow::empty_tab_bar()),
    )
    .with_hit_regions(vec![ChromeHitRegion::new(
        BandRect::new(8.0, 0.0, 80.0, 18.0).expect("tab bounds"),
        ChromeAction::Presented {
            interaction: InteractionId::new(4),
        },
    )]);
    let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.presentation_id = PresentationId::new(9);
    frame.frame_chrome = FrameChrome::layout(
        FrameSize::new(800.0, 600.0).expect("frame size"),
        vec![
            ChromeBandRequest::new(
                FrameChromeKind::MenuBar,
                18.0,
                FrameChromeContent::MenuBar(menu),
            ),
            ChromeBandRequest::new(
                FrameChromeKind::ToolBar,
                34.0,
                FrameChromeContent::ToolBar(tool),
            ),
            tab,
        ],
    )
    .expect("frame chrome");

    assert!(matches!(
        frame_chrome_hit(&frame, 20.0, 30.0),
        Some((ChromeAction::InvokeToolBarItem { index: 0 }, bounds))
            if bounds.y() == 18.0
    ));
    assert!(matches!(
        frame_chrome_hit(&frame, 20.0, 56.0),
        Some((ChromeAction::Presented { interaction }, bounds))
            if interaction.get() == 4 && bounds.y() == 52.0
    ));
    frame
        .install_presented_hit_index(
            PresentedHitIndex::from_parts(
                frame.presentation_id,
                vec![PresentedHitRegion::new(
                    None,
                    PresentedRegionKind::TabBar,
                    FrameRect::new(0.0, 52.0, 800.0, 18.0).unwrap(),
                    i32::MAX,
                )],
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    frame
        .install_presented_pointer(
            vec![neomacs_display_protocol::PresentedPointerRegion::new_owned(
                neomacs_display_protocol::PresentedRegionId::new(
                    None,
                    neomacs_display_protocol::PresentedRegionKind::TabBar,
                ),
                neomacs_display_protocol::FrameRect::new(8.0, 52.0, 80.0, 18.0)
                    .expect("pointer bounds"),
                Some(InteractionId::new(12)),
                None,
            )],
            vec![],
        )
        .expect("displayed pointer map");
    let mut app = make_test_app(800, 600, 1.0);
    let Some(primary_frame) = ensure_primary_frame(&mut app) else {
        return;
    };
    primary_frame.compositor.current_frame = Some(frame);
    assert_eq!(app.toolbar_y_origin(), 18.0);
    assert_eq!(app.toolbar_hit_test(20.0, 30.0), Some(0));
    assert_eq!(app.tab_bar_hit_test(20.0, 56.0), Some((9, 12)));
    let hit = app.menu_bar_hit_test(20.0, 10.0).expect("menu hit");
    assert_eq!(hit.index, 0);
    assert_eq!(hit.key, "file");
    assert_eq!(hit.menu_x, 8.0);
    assert_eq!(hit.anchor.x, 8.0);
    assert_eq!(hit.anchor.y, 0.0);
    assert_eq!(hit.anchor.width, 48.0);
    assert_eq!(hit.anchor.height, 18.0);
}

#[test]
fn empty_and_trailing_frame_chrome_space_owns_pointer_input() {
    use neomacs_display_protocol::frame_chrome::{
        ChromeBandRequest, CompactBarContent, FrameChrome, FrameChromeContent, FrameChromeKind,
        FrameSize, MenuBarContent,
    };

    let mut frame = FrameGlyphBuffer::with_size(200.0, 100.0);
    frame.frame_chrome = FrameChrome::layout(
        FrameSize::new(200.0, 100.0).expect("frame size"),
        vec![ChromeBandRequest::new(
            FrameChromeKind::CompactBar,
            20.0,
            FrameChromeContent::CompactBar(CompactBarContent::empty()),
        )],
    )
    .expect("empty compact band");

    assert!(frame_chrome_owns_pointer(&frame, 190.0, 10.0));
    assert!(!frame_chrome_owns_pointer(&frame, 190.0, 50.0));

    frame.frame_chrome = FrameChrome::layout(
        FrameSize::new(200.0, 100.0).expect("frame size"),
        vec![ChromeBandRequest::new(
            FrameChromeKind::MenuBar,
            18.0,
            FrameChromeContent::MenuBar(MenuBarContent::empty()),
        )],
    )
    .expect("empty menu band");

    assert!(frame_chrome_owns_pointer(&frame, 190.0, 10.0));
    assert!(!frame_chrome_owns_pointer(&frame, 190.0, 28.0));
}

// ===================================================================
// translate_key — Function keys
// ===================================================================

#[test]
fn translate_key_f1_through_f12() {
    let expected: Vec<(NamedKey, u32)> = vec![
        (NamedKey::F1, 0xffbe),
        (NamedKey::F2, 0xffbf),
        (NamedKey::F3, 0xffc0),
        (NamedKey::F4, 0xffc1),
        (NamedKey::F5, 0xffc2),
        (NamedKey::F6, 0xffc3),
        (NamedKey::F7, 0xffc4),
        (NamedKey::F8, 0xffc5),
        (NamedKey::F9, 0xffc6),
        (NamedKey::F10, 0xffc7),
        (NamedKey::F11, 0xffc8),
        (NamedKey::F12, 0xffc9),
    ];
    for (named, keysym) in expected {
        assert_eq!(
            RenderApp::translate_key(&Key::Named(named)),
            keysym,
            "F-key mismatch for {:?}",
            named
        );
    }
}

// ===================================================================
// translate_key — Navigation / editing keys
// ===================================================================

#[test]
fn translate_key_navigation_keys() {
    let cases: Vec<(NamedKey, u32)> = vec![
        (NamedKey::Escape, 0xff1b),
        (NamedKey::Enter, 0xff0d),
        (NamedKey::Tab, 0xff09),
        (NamedKey::Backspace, 0xff08),
        (NamedKey::Delete, 0xffff),
        (NamedKey::Insert, 0xff63),
        (NamedKey::Home, 0xff50),
        (NamedKey::End, 0xff57),
        (NamedKey::PageUp, 0xff55),
        (NamedKey::PageDown, 0xff56),
    ];
    for (named, keysym) in cases {
        assert_eq!(
            RenderApp::translate_key(&Key::Named(named)),
            keysym,
            "Navigation key mismatch for {:?}",
            named
        );
    }
}

// ===================================================================
// translate_key — Arrow keys
// ===================================================================

#[test]
fn translate_key_arrow_keys() {
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::ArrowLeft)),
        0xff51
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::ArrowUp)),
        0xff52
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::ArrowRight)),
        0xff53
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::ArrowDown)),
        0xff54
    );
}

// ===================================================================
// translate_key — Space
// ===================================================================

#[test]
fn translate_key_space() {
    assert_eq!(RenderApp::translate_key(&Key::Named(NamedKey::Space)), 0x20);
}

// ===================================================================
// translate_key — Other named keys (PrintScreen, ScrollLock, Pause)
// ===================================================================

#[test]
fn translate_key_other_named() {
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::PrintScreen)),
        0xff61
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::ScrollLock)),
        0xff14
    );
    assert_eq!(
        RenderApp::translate_key(&Key::Named(NamedKey::Pause)),
        0xff13
    );
}

// ===================================================================
// translate_key — Modifier keys should return 0 (suppressed)
// ===================================================================

#[test]
fn translate_key_modifier_keys_suppressed() {
    let modifiers = vec![
        NamedKey::Shift,
        NamedKey::Control,
        NamedKey::Alt,
        NamedKey::Super,
        NamedKey::CapsLock,
        NamedKey::NumLock,
    ];
    for named in modifiers {
        assert_eq!(
            RenderApp::translate_key(&Key::Named(named)),
            0,
            "Modifier {:?} should be suppressed (return 0)",
            named
        );
    }
}

// ===================================================================
// translate_key — Character keys
// ===================================================================

#[test]
fn translate_key_ascii_characters() {
    for ch in 'a'..='z' {
        let key = Key::Character(SmolStr::new(ch.to_string()));
        assert_eq!(
            RenderApp::translate_key(&key),
            ch as u32,
            "Character key mismatch for '{}'",
            ch
        );
    }
}

#[test]
fn translate_key_digit_characters() {
    for ch in '0'..='9' {
        let key = Key::Character(SmolStr::new(ch.to_string()));
        assert_eq!(
            RenderApp::translate_key(&key),
            ch as u32,
            "Digit key mismatch for '{}'",
            ch
        );
    }
}

#[test]
fn translate_key_special_characters() {
    let specials = vec![
        ('!', 0x21),
        ('@', 0x40),
        ('#', 0x23),
        ('/', 0x2f),
        ('-', 0x2d),
        ('=', 0x3d),
        ('[', 0x5b),
        (']', 0x5d),
        (';', 0x3b),
        ('\'', 0x27),
    ];
    for (ch, code) in specials {
        let key = Key::Character(SmolStr::new(ch.to_string()));
        assert_eq!(
            RenderApp::translate_key(&key),
            code,
            "Special char mismatch for '{}'",
            ch
        );
    }
}

#[test]
fn translate_key_unicode_character() {
    // Multi-byte Unicode characters should return the Unicode code point
    let key = Key::Character(SmolStr::new("\u{00e9}")); // e-acute
    assert_eq!(RenderApp::translate_key(&key), 0xe9);

    let key = Key::Character(SmolStr::new("\u{4e2d}")); // CJK character
    assert_eq!(RenderApp::translate_key(&key), 0x4e2d);
}

#[test]
fn translate_key_empty_character_string() {
    let key = Key::Character(SmolStr::new(""));
    assert_eq!(RenderApp::translate_key(&key), 0);
}

// ===================================================================
// translate_key — Unrecognized / dead keys
// ===================================================================

#[test]
fn translate_key_dead_returns_zero() {
    let key = Key::Dead(None);
    assert_eq!(RenderApp::translate_key(&key), 0);
}

#[test]
fn translate_key_unidentified_returns_zero() {
    let key = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);
    assert_eq!(RenderApp::translate_key(&key), 0);
}

#[test]
fn translate_committed_text_prefers_uppercase_ascii_without_command_modifiers() {
    assert_eq!(
        RenderApp::translate_committed_text("A", 0),
        Some(vec!['A' as u32])
    );
}

#[test]
fn translate_committed_text_prefers_shifted_punctuation_without_command_modifiers() {
    assert_eq!(
        RenderApp::translate_committed_text("!", 0),
        Some(vec!['!' as u32])
    );
}

#[test]
fn translate_committed_text_ignores_control_only_text() {
    assert_eq!(RenderApp::translate_committed_text("\u{8}", 0), None);
    assert_eq!(RenderApp::translate_committed_text("\r", 0), None);
}

#[test]
fn named_backspace_does_not_use_control_text_path() {
    assert!(!RenderApp::should_use_committed_text(&Key::Named(
        NamedKey::Backspace
    )));
    assert!(!RenderApp::should_use_committed_text(&Key::Named(
        NamedKey::Delete
    )));
    assert!(RenderApp::should_use_committed_text(&Key::Character(
        "x".into()
    )));
}

#[test]
fn translate_committed_text_skips_command_modified_input() {
    assert_eq!(
        RenderApp::translate_committed_text("x", NEOMACS_META_MASK),
        None
    );
    assert_eq!(
        RenderApp::translate_committed_text("x", NEOMACS_CTRL_MASK),
        None
    );
    assert_eq!(
        RenderApp::translate_committed_text("x", NEOMACS_SUPER_MASK),
        None
    );
}

#[test]
fn translate_control_text_preserves_single_control_bytes() {
    assert_eq!(RenderApp::translate_control_text("\u{0e}"), Some(0x0e)); // C-n
    assert_eq!(RenderApp::translate_control_text("\u{10}"), Some(0x10)); // C-p
    assert_eq!(RenderApp::translate_control_text("\r"), Some(0x0d));
}

#[test]
fn translate_control_text_ignores_printable_and_multi_char_text() {
    assert_eq!(RenderApp::translate_control_text("n"), None);
    assert_eq!(RenderApp::translate_control_text("np"), None);
    assert_eq!(RenderApp::translate_control_text(""), None);
}

// ===================================================================
// detect_resize_edge — decorations enabled (always None)
// ===================================================================

#[test]
fn resize_edge_returns_none_when_decorations_enabled() {
    let app = make_test_app(800, 600, 1.0);
    // Default chrome has decorations_enabled = true
    assert!(
        app.frame_windows
            .primary_window()
            .expect("primary window state")
            .chrome()
            .decorations_enabled
    );
    assert_eq!(app.detect_resize_edge(0.0, 0.0), None);
    assert_eq!(app.detect_resize_edge(400.0, 300.0), None);
}

// ===================================================================
// detect_resize_edge — corners (5px border zone)
// ===================================================================

#[test]
fn resize_edge_top_left_corner() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    assert_eq!(
        app.detect_resize_edge(0.0, 0.0),
        Some(ResizeDirection::NorthWest)
    );
    assert_eq!(
        app.detect_resize_edge(4.9, 4.9),
        Some(ResizeDirection::NorthWest)
    );
}

#[test]
fn resize_edge_top_right_corner() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    // w=800, border=5 => on_right when x >= 795
    assert_eq!(
        app.detect_resize_edge(795.0, 0.0),
        Some(ResizeDirection::NorthEast)
    );
    assert_eq!(
        app.detect_resize_edge(799.0, 4.0),
        Some(ResizeDirection::NorthEast)
    );
}

#[test]
fn resize_edge_bottom_left_corner() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    // h=600, border=5 => on_bottom when y >= 595
    assert_eq!(
        app.detect_resize_edge(0.0, 595.0),
        Some(ResizeDirection::SouthWest)
    );
    assert_eq!(
        app.detect_resize_edge(4.0, 599.0),
        Some(ResizeDirection::SouthWest)
    );
}

#[test]
fn resize_edge_bottom_right_corner() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    assert_eq!(
        app.detect_resize_edge(795.0, 595.0),
        Some(ResizeDirection::SouthEast)
    );
    assert_eq!(
        app.detect_resize_edge(799.0, 599.0),
        Some(ResizeDirection::SouthEast)
    );
}

// ===================================================================
// detect_resize_edge — edges (not corners)
// ===================================================================

#[test]
fn resize_edge_left() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    // Left edge, but not in top or bottom border zone
    assert_eq!(
        app.detect_resize_edge(0.0, 300.0),
        Some(ResizeDirection::West)
    );
    assert_eq!(
        app.detect_resize_edge(4.9, 300.0),
        Some(ResizeDirection::West)
    );
}

#[test]
fn resize_edge_right() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    assert_eq!(
        app.detect_resize_edge(795.0, 300.0),
        Some(ResizeDirection::East)
    );
    assert_eq!(
        app.detect_resize_edge(799.0, 300.0),
        Some(ResizeDirection::East)
    );
}

#[test]
fn resize_edge_top() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    // Top edge, but not in left or right border zone
    assert_eq!(
        app.detect_resize_edge(400.0, 0.0),
        Some(ResizeDirection::North)
    );
    assert_eq!(
        app.detect_resize_edge(400.0, 4.9),
        Some(ResizeDirection::North)
    );
}

#[test]
fn resize_edge_bottom() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    assert_eq!(
        app.detect_resize_edge(400.0, 595.0),
        Some(ResizeDirection::South)
    );
    assert_eq!(
        app.detect_resize_edge(400.0, 599.0),
        Some(ResizeDirection::South)
    );
}

// ===================================================================
// detect_resize_edge — interior (no edge)
// ===================================================================

#[test]
fn resize_edge_interior_returns_none() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    // Center of the window — well inside border zone
    assert_eq!(app.detect_resize_edge(400.0, 300.0), None);
    // Just inside each border
    assert_eq!(app.detect_resize_edge(5.0, 5.0), None);
    assert_eq!(app.detect_resize_edge(794.9, 594.9), None);
}

// ===================================================================
// detect_resize_edge — boundary values at exactly the border threshold
// ===================================================================

#[test]
fn resize_edge_boundary_exact() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    // x=5.0 is NOT on_left (on_left requires x < 5.0)
    assert_eq!(app.detect_resize_edge(5.0, 300.0), None);
    // x=4.999... is still on_left
    assert_eq!(
        app.detect_resize_edge(4.999, 300.0),
        Some(ResizeDirection::West)
    );
    // y=5.0 is NOT on_top
    assert_eq!(app.detect_resize_edge(300.0, 5.0), None);
    // x=795.0 IS on_right (on_right requires x >= 795.0)
    assert_eq!(
        app.detect_resize_edge(795.0, 300.0),
        Some(ResizeDirection::East)
    );
    // x=794.9 is NOT on_right
    assert_eq!(app.detect_resize_edge(794.9, 300.0), None);
    // y=595.0 IS on_bottom
    assert_eq!(
        app.detect_resize_edge(300.0, 595.0),
        Some(ResizeDirection::South)
    );
    // y=594.9 is NOT on_bottom
    assert_eq!(app.detect_resize_edge(300.0, 594.9), None);
}

// ===================================================================
// detect_resize_edge — small window where border zones might overlap
// ===================================================================

#[test]
fn resize_edge_small_window() {
    let mut app = make_test_app(10, 10, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    // At (0,0) — top-left corner (left and top overlap)
    assert_eq!(
        app.detect_resize_edge(0.0, 0.0),
        Some(ResizeDirection::NorthWest)
    );
    // At (9,9) — bottom-right corner
    assert_eq!(
        app.detect_resize_edge(9.0, 9.0),
        Some(ResizeDirection::SouthEast)
    );
    // At (5,5) — the center, which is also exactly at the border threshold
    // on_left = 5 < 5 = false, on_right = 5 >= 5 = true
    // on_top = 5 < 5 = false, on_bottom = 5 >= 5 = true
    assert_eq!(
        app.detect_resize_edge(5.0, 5.0),
        Some(ResizeDirection::SouthEast)
    );
}

// ===================================================================
// titlebar_hit_test — decorations enabled (always 0)
// ===================================================================

#[test]
fn titlebar_returns_zero_when_decorations_enabled() {
    let app = make_test_app(800, 600, 1.0);
    assert!(
        app.frame_windows
            .primary_window()
            .expect("primary window state")
            .chrome()
            .decorations_enabled
    );
    assert_eq!(app.titlebar_hit_test(0.0, 0.0), 0);
    assert_eq!(app.titlebar_hit_test(400.0, 10.0), 0);
}

// ===================================================================
// titlebar_hit_test — fullscreen (always 0)
// ===================================================================

#[test]
fn titlebar_returns_zero_when_fullscreen() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .is_fullscreen = true;
    assert_eq!(app.titlebar_hit_test(400.0, 10.0), 0);
}

// ===================================================================
// titlebar_hit_test — zero titlebar height (always 0)
// ===================================================================

#[test]
fn titlebar_returns_zero_when_height_is_zero() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .titlebar_height = 0.0;
    assert_eq!(app.titlebar_hit_test(400.0, 10.0), 0);
}

#[test]
fn titlebar_returns_zero_when_height_is_negative() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .titlebar_height = -5.0;
    assert_eq!(app.titlebar_hit_test(400.0, 0.0), 0);
}

// ===================================================================
// titlebar_hit_test — below title bar (always 0)
// ===================================================================

#[test]
fn titlebar_returns_zero_below_titlebar() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .titlebar_height = 30.0;
    // y >= titlebar_height means below
    assert_eq!(app.titlebar_hit_test(400.0, 30.0), 0);
    assert_eq!(app.titlebar_hit_test(400.0, 100.0), 0);
}

// ===================================================================
// titlebar_hit_test — button areas
// Window width (logical) = 800 / 1.0 = 800.  btn_w = 46.
//   close:    x >= 800-46  = 754
//   maximize: x >= 800-92  = 708  and x < 754
//   minimize: x >= 800-138 = 662  and x < 708
//   drag:     x < 662
// ===================================================================

#[test]
fn titlebar_close_button() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .titlebar_height = 30.0;
    assert_eq!(app.titlebar_hit_test(754.0, 15.0), 2);
    assert_eq!(app.titlebar_hit_test(799.0, 0.0), 2);
}

#[test]
fn titlebar_maximize_button() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .titlebar_height = 30.0;
    assert_eq!(app.titlebar_hit_test(708.0, 15.0), 3);
    assert_eq!(app.titlebar_hit_test(753.9, 15.0), 3);
}

#[test]
fn titlebar_minimize_button() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .titlebar_height = 30.0;
    assert_eq!(app.titlebar_hit_test(662.0, 15.0), 4);
    assert_eq!(app.titlebar_hit_test(707.9, 15.0), 4);
}

#[test]
fn titlebar_drag_area() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .titlebar_height = 30.0;
    assert_eq!(app.titlebar_hit_test(0.0, 15.0), 1);
    assert_eq!(app.titlebar_hit_test(300.0, 15.0), 1);
    assert_eq!(app.titlebar_hit_test(661.9, 15.0), 1);
}

// ===================================================================
// titlebar_hit_test — with scale_factor > 1
// Logical width = physical_width / scale_factor = 1600 / 2.0 = 800
// So button positions in logical pixels are the same as the 800px case.
// ===================================================================

#[test]
fn titlebar_with_scale_factor() {
    let mut app = make_test_app(1600, 1200, 2.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .titlebar_height = 30.0;
    // Logical width = 1600/2.0 = 800
    // close_x = 800-46 = 754, max_x = 708, min_x = 662
    assert_eq!(app.titlebar_hit_test(760.0, 10.0), 2); // close
    assert_eq!(app.titlebar_hit_test(720.0, 10.0), 3); // maximize
    assert_eq!(app.titlebar_hit_test(670.0, 10.0), 4); // minimize
    assert_eq!(app.titlebar_hit_test(100.0, 10.0), 1); // drag
}

// ===================================================================
// titlebar_hit_test — boundary between buttons
// ===================================================================

#[test]
fn titlebar_button_boundaries() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .titlebar_height = 30.0;
    // Exact boundary: close_x = 754
    assert_eq!(app.titlebar_hit_test(754.0, 15.0), 2); // close
    assert_eq!(app.titlebar_hit_test(753.9, 15.0), 3); // maximize (just left of close)
    // Exact boundary: max_x = 708
    assert_eq!(app.titlebar_hit_test(708.0, 15.0), 3); // maximize
    assert_eq!(app.titlebar_hit_test(707.9, 15.0), 4); // minimize (just left of maximize)
    // Exact boundary: min_x = 662
    assert_eq!(app.titlebar_hit_test(662.0, 15.0), 4); // minimize
    assert_eq!(app.titlebar_hit_test(661.9, 15.0), 1); // drag (just left of minimize)
}

// ===================================================================
// titlebar_hit_test — y boundary at titlebar_height
// ===================================================================

#[test]
fn titlebar_y_boundary() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .titlebar_height = 30.0;
    // Just inside (y=29.9 < 30.0)
    assert_eq!(app.titlebar_hit_test(100.0, 29.9), 1);
    // Exactly at boundary (y=30.0 >= 30.0)
    assert_eq!(app.titlebar_hit_test(100.0, 30.0), 0);
}

// ===================================================================
// titlebar_hit_test — custom titlebar height
// ===================================================================

#[test]
fn titlebar_custom_height() {
    let mut app = make_test_app(800, 600, 1.0);
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .decorations_enabled = false;
    app.frame_windows
        .primary_window_mut()
        .expect("primary window state")
        .chrome_mut()
        .titlebar_height = 50.0;
    // y=49 is in the titlebar
    assert_eq!(app.titlebar_hit_test(100.0, 49.0), 1);
    // y=50 is below
    assert_eq!(app.titlebar_hit_test(100.0, 50.0), 0);
}

// ===================================================================
// TITLEBAR_BUTTON_WIDTH constant
// ===================================================================

#[test]
fn titlebar_button_width_constant() {
    assert_eq!(RenderApp::TITLEBAR_BUTTON_WIDTH, 46.0);
}
