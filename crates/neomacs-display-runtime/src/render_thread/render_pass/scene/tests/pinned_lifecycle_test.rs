//! Bounded native-size and isolated two-device pinned-picture lifetimes.
use super::super::super::{composition_targets as targets, full_render::place_pane_composition};
use super::native_boundary::full_plan;
use super::pane_size_boundary::install_pane_change;
use super::*;
use crate::render_thread::frame_compositor::continuity::pane_layout::PixelGrid;
use crate::render_thread::frame_compositor::layout_driver::{LayoutDriver, OutgoingPicture};
use crate::render_thread::frame_windows::{
    FrameLifecycle, GuiFrameWindowManager, GuiFrameWindowState,
};
use neomacs_display_protocol::{DeviceScale, FrameGlyph, SurfaceState};
use neomacs_renderer_wgpu::renderer::{NativeContentPlacement, RenderTarget};

fn pinned(
    renderer: &mut WgpuRenderer,
    width: u32,
) -> (
    GuiFrameRenderState,
    FrameGlyphBuffer,
    neomacs_display_protocol::frame_time::EventTime,
) {
    let origin = observe_platform_now();
    let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, origin);
    let (mut before, after) = install_pane_change(&mut render, origin, 1);
    render.set_surface_state(
        SurfaceState::from_device_size(width, H, DeviceScale::new(1.0).unwrap()).unwrap(),
    );
    before.width = width as f32;
    before.background_alpha = 1.0;
    before.glyphs.clear();
    before.glyphs.push(FrameGlyph::Background {
        bounds: neomacs_display_protocol::Rect::new(0.0, 0.0, 64.0, H as f32),
        color: Color::RED,
    });
    before.glyphs.push(FrameGlyph::Background {
        bounds: neomacs_display_protocol::Rect::new(64.0, 0.0, width as f32 - 64.0, H as f32),
        color: Color::BLUE,
    });
    let old = render
        .compositor
        .transitions
        .advance_compositions(renderer, SnapshotSize::new(width, H).unwrap())
        .unwrap();
    renderer.render_frame_glyphs(
        old.view(),
        &before,
        render.compositor.glyph_atlas.as_mut().unwrap(),
        mapping(&before, width, H),
        false,
        None,
        None,
        None,
        None,
        None,
    );
    drop(old);
    renderer.set_frame_sample(FrameSample::new(
        origin.plus(Duration::from_millis(250)),
        Duration::ZERO,
    ));
    let c = render.sample_pane_layout(
        &super::super::super::surface::SurfaceAcquired::for_test(),
        renderer.frame_sample(),
        PixelGrid::new(1.0),
    );
    let new = render
        .compositor
        .transitions
        .advance_compositions(renderer, SnapshotSize::new(width, H).unwrap())
        .unwrap();
    renderer.render_frame_glyphs(
        new.view(),
        &after,
        render.compositor.glyph_atlas.as_mut().unwrap(),
        mapping(&after, width, H),
        false,
        None,
        None,
        None,
        None,
        None,
    );
    let out = target(renderer);
    place_pane_composition(
        renderer,
        &mut render,
        &new,
        &out.create_view(&Default::default()),
        (width as f32, H as f32),
        &c.blits,
    );
    drop(new);
    assert!(matches!(
        &render.compositor.layout,
        LayoutDriver::Animating {
            outgoing: OutgoingPicture::Pinned(Some(_)),
            ..
        }
    ));
    (render, after, origin)
}
fn draw_tick(
    renderer: &mut WgpuRenderer,
    render: &mut GuiFrameRenderState,
    root: &FrameGlyphBuffer,
    origin: neomacs_display_protocol::frame_time::EventTime,
) -> Vec<u8> {
    renderer.set_frame_sample(FrameSample::new(
        origin.plus(Duration::from_millis(500)),
        Duration::ZERO,
    ));
    let t = targets::prepare_frame_targets(
        renderer,
        render,
        mapping(root, W, H).surface(),
        &full_plan(),
    )
    .unwrap();
    let c = render.sample_pane_layout(
        &super::super::super::surface::SurfaceAcquired::for_test(),
        renderer.frame_sample(),
        PixelGrid::new(1.0),
    );
    eprintln!("lifecycle pane blits {:?}", c.blits);
    let new = render
        .compositor
        .transitions
        .advance_compositions(renderer, SnapshotSize::new(W, H).unwrap())
        .unwrap();
    draw_root(renderer, render, new.view(), root);
    let out = target(renderer);
    let view = out.create_view(&Default::default());
    place_pane_composition(
        renderer,
        render,
        &new,
        t.native.as_ref().map_or(&view, |n| n.view()),
        (W as f32, H as f32),
        &c.blits,
    );
    if let Some(n) = &t.native {
        renderer.place_native_content_with_opacity(
            NativeContentPlacement::new(RenderTarget::new(&view, mapping(root, W, H).surface()), n)
                .unwrap(),
            root.background,
            1.0,
        );
    }
    pixels(renderer, &out)
}
#[test]
fn native_resize_retires_incompatible_pinned_picture_not_interaction() {
    let mut renderer = WgpuRenderer::new(None, 192, H).expect("real GPU required");
    let (mut render, root, origin) = pinned(&mut renderer, 192);
    let interaction = format!("{:?}", render.compositor.interaction);
    // Equal geometry is not permission to discard a still-readable live pin.
    let id = match &render.compositor.layout {
        LayoutDriver::Animating {
            outgoing: OutgoingPicture::Pinned(Some(p)),
            ..
        } => p.id(),
        _ => unreachable!(),
    };
    render.set_surface_state(
        SurfaceState::from_device_size(192, H, DeviceScale::new(1.0).unwrap()).unwrap(),
    );
    assert!(
        matches!(&render.compositor.layout,LayoutDriver::Animating{outgoing:OutgoingPicture::Pinned(Some(p)),..} if p.id()==id)
    );
    renderer.resize(W, H);
    render.set_surface_state(
        SurfaceState::from_device_size(W, H, DeviceScale::new(1.0).unwrap()).unwrap(),
    );
    crate::render_thread::transitions::clear_frame_transition_textures(
        &mut render.compositor.transitions,
    );
    assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
    let data = draw_tick(&mut renderer, &mut render, &root, origin);
    eprintln!(
        "resized old-only witness x40 {:?} x56 {:?}",
        px(&data, 40, 40),
        px(&data, 56, 40)
    );
    assert!(
        matches!(
            &render.compositor.layout,
            LayoutDriver::Animating {
                outgoing: OutgoingPicture::Pinned(None),
                ..
            }
        ),
        "incompatible source must not survive resize or sample old texture with current UVs"
    );
    assert_eq!(
        px(&data, 40, 40),
        [0, 255, 0, 255],
        "new-picture fallback must not mis-sample old red/blue stripes"
    );
    renderer.set_frame_sample(FrameSample::new(
        origin.plus(Duration::from_secs(2)),
        Duration::ZERO,
    ));
    assert!(
        targets::prepare_frame_targets(
            &mut renderer,
            &mut render,
            mapping(&root, W, H).surface(),
            &full_plan()
        )
        .unwrap()
        .native
        .is_none()
    );
    assert!(
        render
            .sample_pane_layout(
                &super::super::super::surface::SurfaceAcquired::for_test(),
                renderer.frame_sample(),
                PixelGrid::new(1.0)
            )
            .projection
            .is_some()
    );
}
#[test]
fn recreated_device_has_no_old_pane_bindings_and_keeps_cpu_scene() {
    let mut a = WgpuRenderer::new(None, W, H).expect("real device A required");
    let (render, root, origin) = pinned(&mut a, W);
    let interaction = format!("{:?}", render.compositor.interaction);
    let current = render.compositor.current_frame.clone();
    let mut manager = GuiFrameWindowManager::new();
    manager.set_primary_pending(GuiFrameWindowState {
        pending_scale_factor: None,
        lifecycle: FrameLifecycle::Pending {
            width: W,
            height: H,
            scale_factor: 1.0,
            mouse_hidden_for_typing: false,
            ime_enabled: false,
            last_ime_cursor_area: None,
            chrome: Default::default(),
            geometry_hints: None,
        },
        render,
    });
    drop(a);
    manager.clear_gpu_resident_state();
    let mut b = WgpuRenderer::new(None, W, H).expect("real device B required");
    let render = &mut manager.primary_window_mut().unwrap().render;
    assert_eq!(render.compositor.current_frame, current);
    assert_eq!(format!("{:?}", render.compositor.interaction), interaction);
    render.populate_glyph_atlas(b.device(), 1.0);
    // On the predecessor this actually binds an A-owned pin to B, and GPU
    // validation fails; do not replace this with a source-only assertion.
    let data = draw_tick(&mut b, render, &root, origin);
    assert!(matches!(
        &render.compositor.layout,
        LayoutDriver::Animating {
            outgoing: OutgoingPicture::Pinned(None),
            ..
        }
    ));
    assert_eq!(px(&data, 40, 40), [0, 255, 0, 255]);
}
