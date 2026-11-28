use crate::core::frame_glyphs::{CursorStyle, DisplaySlotId, FrameGlyphBuffer, WindowCursor};
use crate::render_thread::state::{PointerAppearanceState, PresentedAppearanceKey};
use neomacs_display_protocol::frame_chrome::PresentationId;
use neomacs_display_protocol::{
    Color, DisplayWindowId, EffectsConfig, FrameRate, PointerAppearanceId,
};

fn filled_box_frame(effects: EffectsConfig) -> (FrameGlyphBuffer, DisplayWindowId) {
    let window_id = DisplayWindowId::new(1);
    let mut frame = FrameGlyphBuffer::with_size(20.0, 20.0);
    frame.window_cursors.push(WindowCursor {
        window_id,
        slot_id: DisplaySlotId::ZERO,
        x: 1.0,
        y: 2.0,
        width: 3.0,
        height: 4.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
        ascent: 0.0,
        active: true,
        clip_window_id: None,
    });
    frame.set_window_cursor_effects(window_id, effects);
    (frame, window_id)
}

// If this claim is false, a hovered or pressed pointer appearance is composited
// from a retained scene captured before the appearance changed: the cell under
// the pointer keeps its old look until the next scene commit, and the cursor
// cells retained for that scene redraw the stale appearance over the composite.
#[test]
fn a_hovered_or_pressed_pointer_appearance_forces_the_full_render_and_retains_no_cursor_cells() {
    let mut pointer = PointerAppearanceState::default();
    let mut frame = FrameGlyphBuffer::with_size(20.0, 20.0);
    frame.window_cursors.push(WindowCursor {
        window_id: DisplayWindowId::new(1),
        slot_id: DisplaySlotId::ZERO,
        x: 1.0,
        y: 2.0,
        width: 3.0,
        height: 4.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
        ascent: 0.0,
        active: true,
        clip_window_id: None,
    });
    assert!(super::pointer_appearance_allowed(&pointer));
    assert_eq!(
        super::build_filled_box_cursor_cells(&frame, 1.0, &pointer).len(),
        1
    );

    let key = PresentedAppearanceKey::new(
        PresentationId::new(7),
        PointerAppearanceId::try_from(0usize).unwrap(),
    );
    pointer.hover(Some(key));
    assert!(!super::pointer_appearance_allowed(&pointer));
    assert!(super::build_filled_box_cursor_cells(&frame, 1.0, &pointer).is_empty());

    pointer.press();
    assert!(!super::pointer_appearance_allowed(&pointer));
    assert!(super::build_filled_box_cursor_cells(&frame, 1.0, &pointer).is_empty());
}

// If this claim is false, a window with its own cursor-effect profile loses it
// the moment the frame takes the retained-static path: the mini-frame falls back
// to the global profile, so the cursor changes colour-cycle behaviour depending
// on which composition strategy the frame happened to pick.
#[test]
fn a_retained_filled_box_cursor_cell_resolves_the_same_local_effect_profile_as_a_full_render() {
    let pointer = PointerAppearanceState::default();

    for local_enabled in [false, true] {
        let mut local = EffectsConfig::cursor_profile_baseline();
        local.cursor_color_cycle.enabled = local_enabled;
        local.cursor_color_cycle.fps = FrameRate::new(12).unwrap();
        let (frame, window_id) = filled_box_frame(local.clone());

        let mut global = EffectsConfig::default();
        global.cursor_color_cycle.enabled = !local_enabled;
        global.cursor_color_cycle.fps = FrameRate::new(60).unwrap();

        let cells = super::build_filled_box_cursor_cells(&frame, 1.0, &pointer);
        assert_eq!(cells.len(), 1);
        assert_eq!(
            cells[0]
                .mini
                .effective_window_cursor_effects(window_id, &global),
            &local,
            "the retained cursor-only path must resolve the same local profile as the full renderer"
        );
    }
}

/// A root cursor is below child frames in a full render. The retained scene
/// cannot put that cursor back on top of a settled popup during interpolation.
#[test]
fn settled_child_frames_keep_root_cursor_below_their_scene() {
    use crate::render_thread::cursor::CursorTarget;
    use crate::render_thread::frame_sched::RenderWork;
    use crate::render_thread::frame_windows::GuiFrameRenderState;
    use neomacs_display_protocol::DisplayFrameId;
    let now = neomacs_display_protocol::frame_time::observe_platform_now();
    let mut render = GuiFrameRenderState::new_without_device(42, false, now);
    let (mut root, _) = filled_box_frame(EffectsConfig::cursor_profile_baseline());
    root.set_frame_identity(
        DisplayFrameId::new(42),
        DisplayFrameId::new(0),
        0.0,
        0.0,
        i32::MIN,
        false,
        0.0,
        Color::BLACK,
        false,
        1.0,
    );
    render.set_current_frame(Some(root), None, Default::default(), Default::default());
    render.begin_presentable_render(RenderWork::RebuildScene);
    assert!(super::is_eligible(true, &[], &render));
    let mut child = FrameGlyphBuffer::with_size(10.0, 10.0);
    child.set_frame_identity(
        DisplayFrameId::new(43),
        DisplayFrameId::new(42),
        0.0,
        0.0,
        1,
        false,
        0.0,
        Color::BLACK,
        false,
        1.0,
    );
    assert!(render.compositor.child_frames.update_frame(child));
    assert!(!render.compositor.child_frames.has_animation_activity());
    render.cursor.anim_enabled = true;
    let target = CursorTarget {
        window_id: 1,
        x: 1.0,
        y: 2.0,
        width: 3.0,
        height: 4.0,
        style: CursorStyle::FilledBox,
        frame_id: 42,
    };
    render.cursor.set_target(target.clone(), now);
    render
        .cursor
        .set_target(CursorTarget { y: 5.0, ..target }, now);
    assert!(render.tick_cursor_animation(now.plus(std::time::Duration::from_millis(16))));
    assert!(
        !super::is_eligible(true, &[], &render),
        "root cursor composition must not paint above a settled child frame"
    );
    assert!(render.compositor.child_frames.remove_frame(43));
    assert!(
        super::is_eligible(true, &[], &render),
        "closing the popup restores cursor composition"
    );
}

fn gpu_test_target(
    renderer: &neomacs_renderer_wgpu::WgpuRenderer,
    w: u32,
    h: u32,
) -> wgpu::Texture {
    renderer.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("retained-static-test-target"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: renderer.surface_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn gpu_read_pixels(
    renderer: &neomacs_renderer_wgpu::WgpuRenderer,
    tex: &wgpu::Texture,
    w: u32,
    h: u32,
) -> Vec<u8> {
    let bpr = (w * 4).div_ceil(256) * 256;
    let buf = renderer.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("retained-static-readback"),
        size: u64::from(bpr * h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = renderer
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    enc.copy_texture_to_buffer(
        tex.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bpr),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue().submit(Some(enc.finish()));
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = renderer.device().poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    rx.recv().unwrap().unwrap();
    let data = slice.get_mapped_range().unwrap();
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h as usize {
        let start = row * bpr as usize;
        out.extend_from_slice(&data[start..start + (w * 4) as usize]);
    }
    drop(data);
    buf.unmap();
    out
}

fn max_channel_diff(a: &[u8], b: &[u8]) -> i32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| (x as i32 - y as i32).abs())
        .max()
        .unwrap_or(0)
}

fn filled_box_scene_frame(w: f32, h: f32, presentation: u64) -> FrameGlyphBuffer {
    use neomacs_display_protocol::frame_glyphs::FrameGlyph;
    use neomacs_display_protocol::types::{DisplayFrameId, Rect};
    let mut frame = FrameGlyphBuffer::with_size(w, h);
    frame.presentation_id = PresentationId::new(presentation);
    frame.background = Color::rgb(0.10, 0.12, 0.16);
    frame.background_alpha = 1.0;
    frame.font_pixel_size = 14.0;
    frame.char_width = 8.0;
    frame.char_height = 18.0;
    frame.set_frame_identity(
        DisplayFrameId::new(42),
        DisplayFrameId::new(0),
        0.0,
        0.0,
        i32::MIN,
        false,
        0.0,
        frame.background,
        false,
        1.0,
    );
    frame.glyphs.push(FrameGlyph::Background {
        bounds: Rect::new(0.0, 0.0, w, h),
        color: frame.background,
    });
    let slot = DisplaySlotId {
        window_id: DisplayWindowId::new(1),
        row: 1,
        col: 2,
    };
    frame.glyphs.push(FrameGlyph::Stretch {
        window_id: DisplayWindowId::new(1),
        row_role: neomacs_display_protocol::GlyphRowRole::Text,
        clip_rect: None,
        slot_id: slot,
        x: 16.0,
        y: 30.0,
        width: 8.0,
        height: 18.0,
        bg: Color::rgb(0.20, 0.25, 0.35),
        face_id: neomacs_display_protocol::types::FaceId::new(0),
        bidi_level: 0,
        box_vertical_edges: Default::default(),
    });
    frame.window_cursors.push(WindowCursor {
        window_id: DisplayWindowId::new(1),
        slot_id: slot,
        x: 16.0,
        y: 30.0,
        width: 8.0,
        height: 18.0,
        style: CursorStyle::FilledBox,
        color: Color::rgb(0.95, 0.75, 0.20),
        cursor_fg: Color::rgb(0.05, 0.05, 0.05),
        ascent: 14.0,
        active: true,
        clip_window_id: None,
    });
    frame
}

fn gpu_mapping(
    frame: &FrameGlyphBuffer,
    w: u32,
    h: u32,
) -> neomacs_display_protocol::PresentMapping {
    use neomacs_display_protocol::{
        DeviceScale, GeometrySize, LogicalPixels, PresentMapping, PresentationExtent, SurfaceState,
    };
    let SurfaceState::Drawable(surface) =
        SurfaceState::from_device_size(w, h, DeviceScale::new(1.0).unwrap()).unwrap()
    else {
        unreachable!()
    };
    PresentMapping::top_left_clip(
        surface.content_surface().unwrap(),
        PresentationExtent::new(
            frame.presentation_id,
            GeometrySize::<LogicalPixels>::new(
                LogicalPixels::new(frame.width).unwrap(),
                LogicalPixels::new(frame.height).unwrap(),
            )
            .unwrap(),
        ),
    )
}

/// Mechanism A (#607 Stage 2B): a scene commit followed by cursor-motion
/// `CompositeOnly` ticks for the same generation must run `render_frame_root_glyphs`
/// ONCE (1.00x), not TWICE (2.00x), both without chrome and with static chrome.
/// Reverting Mechanism A causes `root_glyph_passes` to jump from 1 to 2.
#[test]
fn commit_then_cursor_motion_reuses_root_glyph_pass_without_second_build() {
    use super::super::{FrameDrawInputs, NativeDrawContext, full_render, surface::SurfaceAcquired};
    use crate::render_thread::frame_sched::RenderWork;
    use crate::render_thread::frame_stats;
    use crate::render_thread::frame_windows::GuiFrameRenderState;
    use crate::render_thread::state::{ChildFrameStyle, ToolbarResources, WindowChrome};
    use neomacs_display_protocol::types::{AnimatedCursor, DisplayFrameId};
    use neomacs_renderer_wgpu::{SnapshotSize, WgpuRenderer};

    const W: u32 = 160;
    const H: u32 = 120;
    let mut renderer = WgpuRenderer::new(None, W, H).expect("real GPU required");
    let now = neomacs_display_protocol::frame_time::observe_platform_now();

    for (label, with_chrome) in [("no-chrome", false), ("with-titlebar-chrome", true)] {
        let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, now);
        let mut frame = filled_box_scene_frame(W as f32, H as f32, 11);
        render.set_current_frame(
            Some(frame.clone()),
            None,
            Default::default(),
            Default::default(),
        );
        render.begin_presentable_render(RenderWork::RebuildScene);

        let chrome = WindowChrome {
            decorations_enabled: !with_chrome,
            is_fullscreen: false,
            titlebar_height: if with_chrome { 24.0 } else { 0.0 },
            title: "Stage2B".to_string(),
            ..WindowChrome::default()
        };
        let native = NativeDrawContext {
            content_size: (W, H),
            scale_factor: 1.0,
            chrome: &chrome,
        };
        let child_style = ChildFrameStyle::default();
        let toolbar = ToolbarResources::default();
        let mapping = gpu_mapping(&frame, W, H);
        let size = SnapshotSize::new(W, H).unwrap();

        let target = gpu_test_target(&renderer, W, H);
        let target_view = target.create_view(&Default::default());
        let composition = render
            .compositor
            .transitions
            .advance_compositions(&mut renderer, size)
            .expect("composition ring slot");

        let before = frame_stats::snapshot();
        let commit_anim = Some(AnimatedCursor {
            window_id: DisplayWindowId::new(1),
            x: 16.0,
            y: 48.0,
            width: 8.0,
            height: 18.0,
            corners: None,
            frame_id: DisplayFrameId::new(42),
        });
        let commit_inputs = FrameDrawInputs {
            present_mapping: mapping,
            root_animated_cursor: commit_anim,
            animated_cursor: commit_anim,
            bg_gradient: None,
            child_frame_style: &child_style,
            scroll_indicators_enabled: false,
            retain_scroll_body: true,
            toolbar: &toolbar,
        };
        full_render::through_composition_ring(
            &SurfaceAcquired::for_test(),
            &mut renderer,
            native,
            &mut render,
            &composition,
            &target_view,
            &mut frame,
            &commit_inputs,
            true,
            true,
            &[],
        )
        .expect("commit frame draw");

        // Now run 3 cursor-motion CompositeOnly ticks for the SAME scene generation.
        for step in [42.0_f32, 36.0, 30.0] {
            let anim = Some(AnimatedCursor {
                window_id: DisplayWindowId::new(1),
                x: 16.0,
                y: step,
                width: 8.0,
                height: 18.0,
                corners: None,
                frame_id: DisplayFrameId::new(42),
            });
            let inputs = FrameDrawInputs {
                present_mapping: mapping,
                root_animated_cursor: anim,
                animated_cursor: anim,
                bg_gradient: None,
                child_frame_style: &child_style,
                scroll_indicators_enabled: false,
                retain_scroll_body: true,
                toolbar: &toolbar,
            };
            super::draw(
                &mut renderer,
                native,
                &mut render,
                &target_view,
                &frame,
                &inputs,
                true,
                None,
            )
            .expect("composite-only frame draw");
        }
        let after = frame_stats::snapshot();
        let glyph_passes = after.root_glyph_passes - before.root_glyph_passes;
        assert_eq!(
            glyph_passes, 1,
            "{label}: commit + 3 cursor-motion ticks for one generation must run exactly 1 root glyph pass (not 2x), got {glyph_passes}"
        );
        // Pin SharedRoot GPU-budget ownership: when no static chrome is
        // present, RetainedStatic stays SharedRoot (charged only in
        // SnapshotPool::pooled_bytes, not double-counted in unpooled_bytes).
        super::super::composition_targets::report_unpooled_gpu_textures(&mut renderer, &render);
        if !with_chrome {
            assert!(
                render
                    .compositor
                    .retained_static
                    .as_ref()
                    .and_then(|rs| rs.unpooled_texture())
                    .is_none(),
                "{label}: no-chrome commit must keep RetainedStatic as SharedRoot (zero unpooled double-count)"
            );
        } else {
            assert!(
                render
                    .compositor
                    .retained_static
                    .as_ref()
                    .and_then(|rs| rs.unpooled_texture())
                    .is_some(),
                "{label}: chrome-bearing commit must upgrade SharedRoot to Owned on CompositeOnly"
            );
        }

        // Verify final settled frame matches a fresh full render with cursor + chrome.
        let composite_pixels = gpu_read_pixels(&renderer, &target, W, H);
        let ref_tex = gpu_test_target(&renderer, W, H);
        let ref_view = ref_tex.create_view(&Default::default());
        let settled_inputs = FrameDrawInputs {
            present_mapping: mapping,
            root_animated_cursor: None,
            animated_cursor: None,
            bg_gradient: None,
            child_frame_style: &child_style,
            scroll_indicators_enabled: false,
            retain_scroll_body: true,
            toolbar: &toolbar,
        };
        super::super::render_frame_window_contents(
            &mut renderer,
            native,
            &mut render,
            &ref_view,
            &frame,
            &settled_inputs,
            true,
            true,
        )
        .expect("reference full render");
        let ref_pixels = gpu_read_pixels(&renderer, &ref_tex, W, H);
        let diff = max_channel_diff(&composite_pixels, &ref_pixels);
        assert!(
            diff <= 2,
            "{label}: reused retained scene + cursor must match full render, max_diff={diff}"
        );
    }

    // Also verify the direct `onto_target` path (when offscreen composition is
    // disabled but cursor motion is animating) reuses a single root glyph pass.
    {
        use crate::render_thread::cursor::CursorTarget;
        let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, now);
        let mut frame = filled_box_scene_frame(W as f32, H as f32, 12);
        render.set_current_frame(
            Some(frame.clone()),
            None,
            Default::default(),
            Default::default(),
        );
        render.begin_presentable_render(RenderWork::RebuildScene);
        render.cursor.anim_enabled = true;
        let start_target = CursorTarget {
            window_id: 1,
            x: 16.0,
            y: 48.0,
            width: 8.0,
            height: 18.0,
            style: CursorStyle::FilledBox,
            frame_id: 42,
        };
        render.cursor.set_target(start_target.clone(), now);
        render.cursor.set_target(
            CursorTarget {
                y: 30.0,
                ..start_target
            },
            now,
        );
        assert!(render.cursor.is_animating());

        let chrome = WindowChrome::default();
        let native = NativeDrawContext {
            content_size: (W, H),
            scale_factor: 1.0,
            chrome: &chrome,
        };
        let child_style = ChildFrameStyle::default();
        let toolbar = ToolbarResources::default();
        let mapping = gpu_mapping(&frame, W, H);
        let target = gpu_test_target(&renderer, W, H);
        let target_view = target.create_view(&Default::default());

        let before = frame_stats::snapshot();
        let commit_anim = render.cursor.animated_cursor();
        let commit_inputs = FrameDrawInputs {
            present_mapping: mapping,
            root_animated_cursor: commit_anim,
            animated_cursor: commit_anim,
            bg_gradient: None,
            child_frame_style: &child_style,
            scroll_indicators_enabled: false,
            retain_scroll_body: true,
            toolbar: &toolbar,
        };
        full_render::onto_target(
            &SurfaceAcquired::for_test(),
            &mut renderer,
            native,
            &mut render,
            &target_view,
            &mut frame,
            &commit_inputs,
            true,
            true,
        )
        .expect("onto_target animating commit draw");
        for step in [40.0_f32, 34.0, 30.0] {
            let anim = Some(AnimatedCursor {
                window_id: DisplayWindowId::new(1),
                x: 16.0,
                y: step,
                width: 8.0,
                height: 18.0,
                corners: None,
                frame_id: DisplayFrameId::new(42),
            });
            let inputs = FrameDrawInputs {
                present_mapping: mapping,
                root_animated_cursor: anim,
                animated_cursor: anim,
                bg_gradient: None,
                child_frame_style: &child_style,
                scroll_indicators_enabled: false,
                retain_scroll_body: true,
                toolbar: &toolbar,
            };
            super::draw(
                &mut renderer,
                native,
                &mut render,
                &target_view,
                &frame,
                &inputs,
                true,
                None,
            )
            .expect("onto_target follow-up composite draw");
        }
        let after = frame_stats::snapshot();
        let glyph_passes = after.root_glyph_passes - before.root_glyph_passes;
        assert_eq!(
            glyph_passes, 1,
            "onto_target animating commit + 3 cursor-motion ticks must run exactly 1 root glyph pass, got {glyph_passes}"
        );
    }
}

/// Mechanism B (#607 Stage 2B): `retained_static::draw`'s composite phase
/// (blit retained scene + cursor layer + filled-box cell redraws) must issue
/// exactly 1 `wgpu::Queue::submit`, not 3. Reverting Mechanism B jumps the
/// submit count from 1 to 3 even while Mechanism A remains intact.
#[test]
fn retained_static_draw_coalesces_composite_to_single_queue_submit() {
    use super::super::{FrameDrawInputs, NativeDrawContext};
    use crate::render_thread::frame_sched::RenderWork;
    use crate::render_thread::frame_stats;
    use crate::render_thread::frame_windows::GuiFrameRenderState;
    use crate::render_thread::state::{ChildFrameStyle, ToolbarResources, WindowChrome};
    use neomacs_display_protocol::types::{AnimatedCursor, DisplayFrameId};
    use neomacs_renderer_wgpu::WgpuRenderer;

    const W: u32 = 160;
    const H: u32 = 120;
    let mut renderer = WgpuRenderer::new(None, W, H).expect("real GPU required");
    let now = neomacs_display_protocol::frame_time::observe_platform_now();
    let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, now);
    let frame = filled_box_scene_frame(W as f32, H as f32, 17);
    render.set_current_frame(
        Some(frame.clone()),
        None,
        Default::default(),
        Default::default(),
    );
    render.begin_presentable_render(RenderWork::RebuildScene);

    let chrome = WindowChrome::default();
    let native = NativeDrawContext {
        content_size: (W, H),
        scale_factor: 1.0,
        chrome: &chrome,
    };
    let child_style = ChildFrameStyle::default();
    let toolbar = ToolbarResources::default();
    let mapping = gpu_mapping(&frame, W, H);

    let target = gpu_test_target(&renderer, W, H);
    let target_view = target.create_view(&Default::default());

    // Warm the retained-static texture on call 1 so subsequent calls isolate
    // ONLY the composite phase (orthogonal to Mechanism A).
    let warm_inputs = FrameDrawInputs {
        present_mapping: mapping,
        root_animated_cursor: None,
        animated_cursor: None,
        bg_gradient: None,
        child_frame_style: &child_style,
        scroll_indicators_enabled: false,
        retain_scroll_body: true,
        toolbar: &toolbar,
    };
    super::draw(
        &mut renderer,
        native,
        &mut render,
        &target_view,
        &frame,
        &warm_inputs,
        true,
        None,
    )
    .expect("warm retained_static");

    for (label, anim) in [
        ("settled-filled-box", None),
        (
            "in-flight-filled-box",
            Some(AnimatedCursor {
                window_id: DisplayWindowId::new(1),
                x: 16.0,
                y: 44.0,
                width: 8.0,
                height: 18.0,
                corners: None,
                frame_id: DisplayFrameId::new(42),
            }),
        ),
    ] {
        let inputs = FrameDrawInputs {
            present_mapping: mapping,
            root_animated_cursor: anim,
            animated_cursor: anim,
            bg_gradient: None,
            child_frame_style: &child_style,
            scroll_indicators_enabled: false,
            retain_scroll_body: true,
            toolbar: &toolbar,
        };
        let submits_before = renderer.queue_submit_count();
        let stats_before = frame_stats::snapshot();
        super::draw(
            &mut renderer,
            native,
            &mut render,
            &target_view,
            &frame,
            &inputs,
            true,
            None,
        )
        .expect("coalesced composite draw");
        let submits_delta = renderer.queue_submit_count() - submits_before;
        let stats_after = frame_stats::snapshot();
        let composite_submits = stats_after.retained_composite_queue_submits
            - stats_before.retained_composite_queue_submits;
        assert_eq!(
            submits_delta, 1,
            "{label}: retained_static::draw must coalesce blit + cursor + filled-box cell into 1 queue.submit, got {submits_delta}"
        );
        assert_eq!(
            composite_submits, 1,
            "{label}: RETAINED_COMPOSITE_QUEUE_SUBMITS must record 1 submit per frame, got {composite_submits}"
        );

        let actual = gpu_read_pixels(&renderer, &target, W, H);
        let ref_tex = gpu_test_target(&renderer, W, H);
        let ref_view = ref_tex.create_view(&Default::default());
        renderer.render_frame_glyphs(
            &ref_view,
            &frame,
            render.compositor.glyph_atlas.as_mut().unwrap(),
            mapping,
            true,
            anim,
            None,
            None,
            None,
            None,
        );
        let expected = gpu_read_pixels(&renderer, &ref_tex, W, H);
        let diff = max_channel_diff(&actual, &expected);
        assert!(
            diff <= 2,
            "{label}: coalesced composite pixels must match full render, max_diff={diff}"
        );
    }
}

/// Helper: count bright-magenta cursor pixels inside vs outside `clip_y_top..clip_y_bottom`.
fn count_magenta_cursor_pixels(
    pixels: &[u8],
    w: u32,
    h: u32,
    clip_y_top: u32,
    clip_y_bottom: u32,
) -> (usize, usize, usize) {
    let mut inside = 0usize;
    let mut leaked_above = 0usize;
    let mut leaked_below = 0usize;
    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            let [r, g, b, _a] = [
                pixels[idx],
                pixels[idx + 1],
                pixels[idx + 2],
                pixels[idx + 3],
            ];
            if r >= 0xe0 && g <= 0x20 && b >= 0xe0 {
                if y < clip_y_top {
                    leaked_above += 1;
                } else if y >= clip_y_bottom {
                    leaked_below += 1;
                } else {
                    inside += 1;
                }
            }
        }
    }
    (inside, leaked_above, leaked_below)
}

fn tall_cursor_window_info(
    window_id: DisplayWindowId,
    frame_w: f32,
    body_top: f32,
    body_h: f32,
) -> neomacs_display_protocol::frame_glyphs::WindowInfo {
    use neomacs_display_protocol::frame_glyphs::{
        PresentedCellOrigin, PresentedWindowGeometry, PresentedWindowRegions, WindowInfo,
    };
    use neomacs_display_protocol::types::Rect;
    let outer = Rect::new(0.0, 0.0, frame_w, body_top + body_h + 17.0);
    let text_body = Rect::new(8.0, body_top, frame_w - 16.0, body_h);
    WindowInfo {
        window_id,
        buffer_id: 1,
        window_start: 1,
        window_end: 100,
        buffer_size: 100,
        buffer_modiff: Default::default(),
        bounds: outer,
        geometry: PresentedWindowGeometry::Complete {
            cell_origin: PresentedCellOrigin::default(),
            regions: PresentedWindowRegions {
                outer,
                text_body,
                header_line: (body_top > 0.0).then(|| Rect::new(0.0, 0.0, frame_w, body_top)),
                mode_line: Some(Rect::new(0.0, body_top + body_h, frame_w, 17.0)),
                ..PresentedWindowRegions::default()
            },
        },
        line_number_field: None,
        mode_line_height: 17.0,
        header_line_height: body_top,
        tab_line_height: 0.0,
        selected: true,
        is_minibuffer: false,
        char_height: 17.0,
        buffer_name: String::new(),
        buffer_file_name: String::new(),
        modified: false,
    }
}

/// R2-1 (#861): `build_filled_box_cursor_cells` + `composite_retained_scene_with_cursor`
/// must clip overflowing tall `FilledBox` cursors (bottom-overflow, top-vscroll, and
/// decorative visual cursors) strictly to the owning window's `text_body`, matching the
/// full render bit-for-bit with zero cursor pixels over the header line, mode line, or
/// lower split window.
#[test]
fn tall_filled_box_cursor_clips_to_text_body_on_retained_static_composite() {
    use super::super::{FrameDrawInputs, NativeDrawContext};
    use crate::render_thread::frame_sched::RenderWork;
    use crate::render_thread::frame_windows::GuiFrameRenderState;
    use crate::render_thread::state::{ChildFrameStyle, ToolbarResources, WindowChrome};
    use neomacs_display_protocol::types::DisplayFrameId;
    use neomacs_renderer_wgpu::WgpuRenderer;

    const W: u32 = 160;
    const H: u32 = 120;
    const BODY_TOP: u32 = 17;
    const BODY_H: u32 = 60;
    const BODY_BOTTOM: u32 = BODY_TOP + BODY_H;

    let mut renderer = WgpuRenderer::new(None, W, H).expect("real GPU required");
    renderer.effects.cursor_color_cycle.enabled = false;
    let now = neomacs_display_protocol::frame_time::observe_platform_now();
    let wid = DisplayWindowId::new(1);
    let pointer = PointerAppearanceState::default();

    let cases: [(
        &str,
        DisplayWindowId,
        Option<DisplayWindowId>,
        f32,
        f32,
        bool,
    ); 3] = [
        ("bottom-overflow", wid, None, 45.0, 90.0, true),
        (
            "top-vscroll-and-bottom-overflow",
            wid,
            None,
            -10.0,
            100.0,
            true,
        ),
        (
            "decorative-bottom-overflow",
            DisplayWindowId::new(-1_000_000),
            Some(wid),
            45.0,
            90.0,
            false,
        ),
    ];

    for (idx, (label, cursor_wid, clip_wid, cy, ch, active)) in cases.into_iter().enumerate() {
        let mut frame = FrameGlyphBuffer::with_size(W as f32, H as f32);
        frame.presentation_id = PresentationId::new(400 + idx as u64);
        frame.set_frame_identity(
            DisplayFrameId::new(42),
            DisplayFrameId::new(0),
            0.0,
            0.0,
            i32::MIN,
            false,
            0.0,
            Color::rgb(0.95, 0.95, 0.95),
            false,
            1.0,
        );
        frame.background = Color::rgb(0.95, 0.95, 0.95);
        frame.font_pixel_size = 14.0;
        frame.char_height = 17.0;
        frame.window_infos.push(tall_cursor_window_info(
            wid,
            W as f32,
            BODY_TOP as f32,
            BODY_H as f32,
        ));
        frame.window_cursors.push(WindowCursor {
            window_id: cursor_wid,
            slot_id: DisplaySlotId {
                window_id: cursor_wid,
                row: 0,
                col: 0,
            },
            x: 8.0,
            y: cy,
            width: 24.0,
            height: ch,
            style: CursorStyle::FilledBox,
            color: Color::rgb(1.0, 0.0, 1.0),
            cursor_fg: Color::BLACK,
            ascent: 0.0,
            active,
            clip_window_id: clip_wid,
        });

        let cells = super::build_filled_box_cursor_cells(&frame, 1.0, &pointer);
        assert_eq!(cells.len(), 1, "{label}: expected 1 filled-box cursor cell");
        assert_eq!(
            cells[0].mini.window_infos.len(),
            1,
            "{label}: mini-frame must carry owning WindowInfo for cursor_text_body_clip"
        );
        let (_, sy, _, sh) = cells[0].scissor;
        assert!(
            sy >= BODY_TOP && sy + sh <= BODY_BOTTOM,
            "{label}: cell scissor y={sy}..{} must stay within text_body {BODY_TOP}..{BODY_BOTTOM}",
            sy + sh
        );

        let mut render = GuiFrameRenderState::new(42, renderer.device(), 1.0, false, now);
        render.set_current_frame(
            Some(frame.clone()),
            None,
            Default::default(),
            Default::default(),
        );
        render.begin_presentable_render(RenderWork::RebuildScene);
        let chrome = WindowChrome::default();
        let native = NativeDrawContext {
            content_size: (W, H),
            scale_factor: 1.0,
            chrome: &chrome,
        };
        let child_style = ChildFrameStyle::default();
        let toolbar = ToolbarResources::default();
        let mapping = gpu_mapping(&frame, W, H);
        let inputs = FrameDrawInputs {
            present_mapping: mapping,
            root_animated_cursor: None,
            animated_cursor: None,
            bg_gradient: None,
            child_frame_style: &child_style,
            scroll_indicators_enabled: false,
            retain_scroll_body: true,
            toolbar: &toolbar,
        };

        // First draw warms the cursorless retained_static scene + clipped cursor cells;
        // second draw takes the coalesced CompositeOnly path (Phase 3).
        let target = gpu_test_target(&renderer, W, H);
        let target_view = target.create_view(&Default::default());
        super::draw(
            &mut renderer,
            native,
            &mut render,
            &target_view,
            &frame,
            &inputs,
            true,
            None,
        )
        .expect("warm retained_static");
        super::draw(
            &mut renderer,
            native,
            &mut render,
            &target_view,
            &frame,
            &inputs,
            true,
            None,
        )
        .expect("composite-only retained_static");

        let comp_pixels = gpu_read_pixels(&renderer, &target, W, H);
        let (inside, leaked_above, leaked_below) =
            count_magenta_cursor_pixels(&comp_pixels, W, H, BODY_TOP, BODY_BOTTOM);
        assert!(
            inside > 0,
            "{label}: expected cursor pixels inside text_body"
        );
        assert_eq!(
            leaked_above, 0,
            "{label}: composite leaked {leaked_above} cursor pixels above text_body"
        );
        assert_eq!(
            leaked_below, 0,
            "{label}: composite leaked {leaked_below} cursor pixels below text_body"
        );

        let ref_tex = gpu_test_target(&renderer, W, H);
        let ref_view = ref_tex.create_view(&Default::default());
        renderer.render_frame_glyphs(
            &ref_view,
            &frame,
            render.compositor.glyph_atlas.as_mut().unwrap(),
            mapping,
            true,
            None,
            None,
            None,
            None,
            None,
        );
        let full_pixels = gpu_read_pixels(&renderer, &ref_tex, W, H);
        let diff = max_channel_diff(&comp_pixels, &full_pixels);
        assert!(
            diff <= 2,
            "{label}: retained_static composite must match full render bit-for-bit, max_diff={diff}"
        );
    }
}

/// R2-2 & R2-3 (#861): real GPU render readback proves all four cursor shapes
/// (`FilledBox`, `Bar`, `Hbar`, `Hollow`) clip strictly to `text_body` for both
/// regular window cursors and decorative `neomacs-visual-cursors` (synthetic
/// negative `window_id` carrying `clip_window_id`).
#[test]
fn all_four_cursor_shapes_and_decorative_cursors_clip_to_text_body_on_gpu() {
    use neomacs_display_protocol::types::DisplayFrameId;
    use neomacs_renderer_wgpu::WgpuRenderer;

    const W: u32 = 160;
    const H: u32 = 120;
    const BODY_TOP: u32 = 17;
    const BODY_H: u32 = 60;
    const BODY_BOTTOM: u32 = BODY_TOP + BODY_H;

    let mut renderer = WgpuRenderer::new(None, W, H).expect("real GPU required");
    renderer.effects.cursor_color_cycle.enabled = false;
    let mut atlas = neomacs_renderer_wgpu::WgpuGlyphAtlas::new(renderer.device());
    let wid = DisplayWindowId::new(1);

    let shapes = [
        (
            "filled-box",
            CursorStyle::FilledBox,
            45.0_f32,
            90.0_f32,
            true,
        ),
        ("bar", CursorStyle::Bar(3.0), 45.0, 90.0, true),
        // Bottom-overflowing Hbar sits entirely below BODY_BOTTOM and must clip out (0 inside, 0 leaked).
        ("hbar-overflow", CursorStyle::Hbar(4.0), 45.0, 90.0, false),
        // Top-vscrolled tall Hbar whose bottom rests inside BODY_TOP..BODY_BOTTOM draws inside with 0 leaks.
        (
            "hbar-vscroll-in-body",
            CursorStyle::Hbar(4.0),
            -10.0,
            70.0,
            true,
        ),
        ("hollow", CursorStyle::Hollow, 45.0, 90.0, true),
        (
            "hollow-top-vscroll",
            CursorStyle::Hollow,
            -10.0,
            100.0,
            true,
        ),
    ];

    for (owner_label, cursor_wid, clip_wid, active) in [
        ("window-cursor", wid, None, true),
        (
            "decorative-cursor",
            DisplayWindowId::new(-1_000_000),
            Some(wid),
            false,
        ),
    ] {
        for (shape_label, style, cy, ch, expect_inside) in shapes {
            let mut frame = FrameGlyphBuffer::with_size(W as f32, H as f32);
            frame.presentation_id = PresentationId::new(500);
            frame.set_frame_identity(
                DisplayFrameId::new(42),
                DisplayFrameId::new(0),
                0.0,
                0.0,
                i32::MIN,
                false,
                0.0,
                Color::rgb(0.95, 0.95, 0.95),
                false,
                1.0,
            );
            frame.background = Color::rgb(0.95, 0.95, 0.95);
            frame.font_pixel_size = 14.0;
            frame.char_height = 17.0;
            frame.window_infos.push(tall_cursor_window_info(
                wid,
                W as f32,
                BODY_TOP as f32,
                BODY_H as f32,
            ));
            frame.window_cursors.push(WindowCursor {
                window_id: cursor_wid,
                slot_id: DisplaySlotId {
                    window_id: cursor_wid,
                    row: 0,
                    col: 0,
                },
                x: 16.0,
                y: cy,
                width: 24.0,
                height: ch,
                style,
                color: Color::rgb(1.0, 0.0, 1.0),
                cursor_fg: Color::BLACK,
                ascent: 0.0,
                active,
                clip_window_id: clip_wid,
            });
            let mapping = gpu_mapping(&frame, W, H);
            let tex = gpu_test_target(&renderer, W, H);
            let view = tex.create_view(&Default::default());
            renderer.render_frame_glyphs(
                &view, &frame, &mut atlas, mapping, true, None, None, None, None, None,
            );
            let pixels = gpu_read_pixels(&renderer, &tex, W, H);
            let (inside, leaked_above, leaked_below) =
                count_magenta_cursor_pixels(&pixels, W, H, BODY_TOP, BODY_BOTTOM);
            if expect_inside {
                assert!(
                    inside > 0,
                    "{owner_label}/{shape_label}: expected cursor pixels inside text_body"
                );
            } else {
                assert_eq!(
                    inside, 0,
                    "{owner_label}/{shape_label}: bottom-overflowing hbar should clip completely"
                );
            }
            assert_eq!(
                leaked_above, 0,
                "{owner_label}/{shape_label}: leaked {leaked_above} pixels above text_body"
            );
            assert_eq!(
                leaked_below, 0,
                "{owner_label}/{shape_label}: leaked {leaked_below} pixels below text_body"
            );
        }
    }
}
