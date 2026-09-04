use super::*;
use crate::core::frame_glyphs::{
    BufferTransitionTarget, ContentTransitionHint, CursorStyle, DisplaySlotId, FrameGlyph,
    FrameGlyphBuffer, PresentedWindowRegions, WindowCursor, WindowEffectHint,
};
use crate::render_thread::cursor::CursorTarget;
#[cfg(feature = "neo-term")]
use crate::render_thread::terminal_expansion::TerminalExpansionUpdate;
use neomacs_display_protocol::types::Color;
use neovm_core::window::GuiFrameGeometryHints;

// =======================================================================
// Helper: create a FrameGlyphBuffer with specified identity fields
// =======================================================================

fn default_geometry_hints() -> GuiFrameGeometryHints {
    GuiFrameGeometryHints {
        base_width: 24,
        base_height: 16,
        min_width: 24,
        min_height: 16,
        width_inc: 8,
        height_inc: 16,
    }
}

#[test]
fn gui_text_input_policy_enables_native_ime_on_window_creation() {
    let policy = NativeTextInputPolicy::for_gui_frame();

    assert!(policy.ime_allowed_on_create);
    assert_eq!(
        policy.initial_cursor_area,
        ImeCursorArea {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        }
    );
}

#[cfg(feature = "neo-term")]
#[test]
fn terminal_expansion_replacement_is_atomic_and_invalidates_the_scene() {
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let mut editor_frame = make_frame(0x42, 0);
    let editor_glyph = FrameGlyph::Border {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(1),
        row_role: neomacs_display_protocol::frame_glyphs::GlyphRowRole::Text,
        clip_rect: None,
        x: 0.0,
        y: 0.0,
        width: 8.0,
        height: 19.0,
        color: Color::WHITE,
    };
    editor_frame.glyphs.push(editor_glyph.clone());
    render.set_current_frame(Some(editor_frame), None, Default::default());
    let face_id = neomacs_display_protocol::types::FaceId::new(0xffff_fff0);
    let generated_glyph = FrameGlyph::Border {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(1),
        row_role: neomacs_display_protocol::frame_glyphs::GlyphRowRole::Text,
        clip_rect: None,
        x: 8.0,
        y: 0.0,
        width: 9.0,
        height: 19.0,
        color: Color::WHITE,
    };
    let generated_faces =
        HashMap::from([(face_id, neomacs_display_protocol::face::Face::new(face_id))]);
    let expansion = TerminalExpansion::new(vec![generated_glyph], generated_faces);
    let initial_generation = render.compositor.current_scene_generation;

    assert_eq!(
        render.replace_terminal_expansion(expansion.clone()),
        TerminalExpansionUpdate::Replaced
    );
    assert!(render.compositor.dirty);
    assert_ne!(
        render.compositor.current_scene_generation,
        initial_generation
    );
    // The editor snapshot remains immutable; terminal state is composed only
    // into the frame exposed for rendering.
    assert_eq!(
        render
            .compositor
            .current_frame
            .as_ref()
            .unwrap()
            .glyphs
            .len(),
        1
    );
    let composed = render.current_frame_clone().expect("composed frame");
    assert_eq!(
        composed.glyphs,
        vec![editor_glyph.clone(), expansion.glyphs()[0].clone()]
    );
    assert!(composed.faces.contains_key(&face_id));

    render.begin_presentable_render();
    let installed_generation = render.compositor.current_scene_generation;
    assert_eq!(
        render.replace_terminal_expansion(expansion),
        TerminalExpansionUpdate::Unchanged
    );
    assert!(!render.compositor.dirty);
    assert_eq!(
        render.compositor.current_scene_generation,
        installed_generation
    );

    assert_eq!(
        render.replace_terminal_expansion(TerminalExpansion::default()),
        TerminalExpansionUpdate::Replaced
    );
    assert!(render.compositor.dirty);
    assert_ne!(
        render.compositor.current_scene_generation,
        installed_generation
    );
    let composed = render.current_frame_clone().expect("editor-only frame");
    assert_eq!(composed.glyphs, vec![editor_glyph]);
    assert!(!composed.faces.contains_key(&face_id));
}

#[cfg(feature = "neo-term")]
#[test]
fn terminal_expansion_rejects_editor_face_collisions_without_partial_installation() {
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let editor_face_id = neomacs_display_protocol::types::FaceId::new(0xffff_fff0);
    let retained_face_id = neomacs_display_protocol::types::FaceId::new(0xffff_fff1);
    let mut editor_frame = make_frame(0x42, 0);
    editor_frame.faces.insert(
        editor_face_id,
        neomacs_display_protocol::face::Face::new(editor_face_id),
    );
    render.set_current_frame(Some(editor_frame), None, Default::default());
    let retained = TerminalExpansion::new(
        Vec::new(),
        HashMap::from([(
            retained_face_id,
            neomacs_display_protocol::face::Face::new(retained_face_id),
        )]),
    );
    assert_eq!(
        render.replace_terminal_expansion(retained),
        TerminalExpansionUpdate::Replaced
    );
    render.begin_presentable_render();
    let installed_generation = render.compositor.current_scene_generation;

    let collision = TerminalExpansion::new(
        Vec::new(),
        HashMap::from([(
            editor_face_id,
            neomacs_display_protocol::face::Face::new(editor_face_id),
        )]),
    );
    assert_eq!(
        render.replace_terminal_expansion(collision),
        TerminalExpansionUpdate::FaceIdCollision(editor_face_id)
    );
    assert!(!render.compositor.dirty);
    assert_eq!(
        render.compositor.current_scene_generation,
        installed_generation
    );
    let composed = render.current_frame_clone().expect("composed frame");
    assert!(composed.faces.contains_key(&editor_face_id));
    assert!(composed.faces.contains_key(&retained_face_id));
}

fn make_frame(frame_id: u64, parent_id: u64) -> FrameGlyphBuffer {
    let mut buf = FrameGlyphBuffer::with_size(800.0, 600.0);
    buf.set_frame_identity(
        neomacs_display_protocol::types::DisplayFrameId::new(frame_id),
        neomacs_display_protocol::types::DisplayFrameId::new(parent_id),
        0.0,
        0.0,
        0,
        false,
        0.0,
        Color::BLACK,
        false,
        1.0,
    );
    buf
}

#[cfg(feature = "video")]
fn make_video_frame(frame_id: u64, parent_id: u64, video_id: u32) -> FrameGlyphBuffer {
    let mut frame = make_frame(frame_id, parent_id);
    frame.add_video(
        neomacs_display_protocol::types::VideoId::new(video_id),
        0.0,
        0.0,
        16.0,
        16.0,
    );
    frame
}

#[cfg(feature = "video")]
#[test]
fn accepted_root_and_child_presentations_own_the_video_visibility_index() {
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let first = neomacs_display_protocol::types::VideoId::new(7);
    let child_video = neomacs_display_protocol::types::VideoId::new(8);

    render.set_current_frame(
        Some(make_video_frame(0x42, 0, first.get())),
        None,
        Default::default(),
    );
    assert!(render.presents_video(first));
    assert!(!render.presents_video(child_video));

    render.update_child_frame(make_video_frame(0x99, 0x42, child_video.get()));
    assert!(render.presents_video(first));
    assert!(render.presents_video(child_video));

    render.remove_child_frame(0x99);
    assert!(render.presents_video(first));
    assert!(!render.presents_video(child_video));

    render.set_current_frame(Some(make_frame(0x42, 0)), None, Default::default());
    assert!(!render.presents_video(first));
}

#[test]
fn root_present_mapping_refreshes_on_surface_and_presentation_edges() {
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let initial_surface =
        SurfaceState::from_device_size(1162, 1194, DeviceScale::new(1.75).unwrap()).unwrap();
    render.set_surface_state(initial_surface);
    assert!(render.present_mapping().is_none());

    let mut stale = FrameGlyphBuffer::with_size(664.0, 682.0);
    stale.presentation_id = PresentationId::new(5);
    render.set_current_frame(Some(stale), None, Default::default());
    let initial = render.present_mapping().unwrap();
    assert_eq!(initial.presentation(), PresentationId::new(5));
    assert_eq!(initial.surface_logical_size().width(), 664.0);

    let maximized =
        SurfaceState::from_device_size(3456, 2125, DeviceScale::new(1.75).unwrap()).unwrap();
    render.set_surface_state(maximized);
    let stale_on_maximized = render.present_mapping().unwrap();
    assert_eq!(stale_on_maximized.presentation(), PresentationId::new(5));
    assert!((stale_on_maximized.surface_logical_size().width() - 1974.8572).abs() < 0.001);
    assert_eq!(
        stale_on_maximized.visible_content_rect().unwrap().width(),
        664.0
    );

    let mut fresh = FrameGlyphBuffer::with_size(1974.8572, 1214.2858);
    fresh.presentation_id = PresentationId::new(6);
    render.set_current_frame(Some(fresh), None, Default::default());
    let fresh_on_maximized = render.present_mapping().unwrap();
    assert_eq!(fresh_on_maximized.presentation(), PresentationId::new(6));
    assert!((fresh_on_maximized.visible_content_rect().unwrap().width() - 1974.8572).abs() < 0.001);
}

#[test]
fn suspended_surface_cannot_retain_a_drawable_present_mapping() {
    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.presentation_id = PresentationId::new(7);
    render.set_current_frame(Some(frame), None, Default::default());
    render.set_surface_state(
        SurfaceState::from_device_size(800, 600, DeviceScale::new(1.0).unwrap()).unwrap(),
    );
    assert!(render.present_mapping().is_some());

    render.set_surface_state(
        SurfaceState::from_device_size(0, 600, DeviceScale::new(1.0).unwrap()).unwrap(),
    );
    assert!(render.present_mapping().is_none());
}

fn set_parent_offset(frame: &mut FrameGlyphBuffer, x: f32, y: f32) {
    let placement = frame.frame_placement;
    frame.frame_placement = neomacs_display_protocol::PresentedFramePlacement::new(
        placement.frame(),
        placement.presentation(),
        placement.parent(),
        neomacs_display_protocol::ParentFrameRect::new(x, y, frame.width, frame.height).unwrap(),
        placement.z_order(),
    );
}

fn install_pointer_region(
    frame: &mut FrameGlyphBuffer,
    interaction: Option<neomacs_display_protocol::InteractionId>,
    visual: bool,
) {
    use neomacs_display_protocol::{
        FaceId, FrameRect, PointerAppearanceId, PointerDrawMode, PresentedPaintSpan,
        PresentedPointerAppearance, PresentedPointerRegion, PresentedPrimitiveKind,
        PresentedRegionId, PresentedRegionKind,
    };

    let appearance = visual.then(|| {
        let face = FaceId::new(7);
        frame.set_face(
            face,
            Color::WHITE,
            None,
            400,
            false,
            0,
            None,
            0,
            None,
            0,
            None,
        );
        frame.add_char('x', 0.0, 0.0, 10.0, 10.0, 8.0, false);
        PresentedPointerAppearance::new(
            vec![PresentedPaintSpan::new(
                PresentedPrimitiveKind::Glyph,
                0,
                1,
                FrameRect::new(0.0, 0.0, 20.0, 20.0).unwrap(),
            )],
            PointerDrawMode::Face(face),
            PointerDrawMode::Face(face),
        )
    });
    frame
        .install_presented_pointer(
            vec![PresentedPointerRegion::new_owned(
                PresentedRegionId::new(None, PresentedRegionKind::TabBar),
                FrameRect::new(0.0, 0.0, 20.0, 20.0).unwrap(),
                interaction,
                appearance
                    .as_ref()
                    .map(|_| PointerAppearanceId::try_from(0usize).unwrap()),
            )],
            appearance.into_iter().collect(),
        )
        .unwrap();
    frame
        .install_presented_hit_index(
            neomacs_display_protocol::PresentedHitIndex::from_parts(
                frame.presentation_id,
                vec![neomacs_display_protocol::PresentedHitRegion::new(
                    None,
                    neomacs_display_protocol::PresentedRegionKind::TabBar,
                    neomacs_display_protocol::FrameRect::new(0.0, 0.0, 20.0, 20.0).unwrap(),
                    0,
                )],
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
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
        label: Some("frame windows test device"),
        ..Default::default()
    }))
    .ok()?;
    Some(device)
}

#[test]
fn secondary_frame_cursor_target_uses_top_level_frame_identity() {
    let mut frame = make_frame(0x42, 0);
    frame.set_phys_cursor(crate::core::frame_glyphs::PhysCursor {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(7),
        charpos: 0,
        row: 0,
        col: 0,
        slot_id: crate::core::frame_glyphs::DisplaySlotId {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(7),
            row: 0,
            col: 0,
        },
        x: 10.0,
        y: 20.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::Bar(2.0),
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
    });

    let target = GuiFrameWindowManager::cursor_target_for_frame(0x42, &frame).expect("cursor");

    assert_eq!(target.frame_id, 0x42);
    assert_eq!(target.window_id, 7);
    assert_eq!(target.x, 10.0);
}

#[test]
fn secondary_frame_cursor_state_clears_when_no_target_remains() {
    let mut state = crate::render_thread::cursor::CursorState::new(
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    state.set_target(
        crate::render_thread::cursor::CursorTarget {
            window_id: 7,
            x: 10.0,
            y: 20.0,
            width: 8.0,
            height: 16.0,
            style: CursorStyle::Bar(2.0),
            frame_id: 0x42,
        },
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );

    state.clear_target();

    assert!(state.target_cloned().is_none());
    assert!(!state.is_animating());
}

#[test]
fn frame_render_state_syncs_visual_cursor_config_from_defaults() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.compositor.visual_cursors.insert(7, {
        let mut cursor = crate::render_thread::cursor::CursorState::new(
            neomacs_display_protocol::frame_time::observe_platform_now(),
        );
        cursor.set_target(
            CursorTarget {
                window_id: 7,
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
                style: CursorStyle::Bar(1.0),
                frame_id: 0x42,
            },
            neomacs_display_protocol::frame_time::observe_platform_now(),
        );
        cursor
    });
    let mut defaults = crate::render_thread::cursor::CursorState::new(
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    defaults.anim_enabled = false;
    defaults.trail_size = 17.0;
    defaults.size_transition_enabled = false;

    render.sync_cursor_config(&defaults, true);

    let visual = render
        .compositor
        .visual_cursors
        .get(&7)
        .expect("visual cursor");
    assert!(!render.cursor.anim_enabled);
    assert!(!visual.anim_enabled);
    assert_eq!(visual.trail_size, 17.0);
    assert!(!visual.size_transition_enabled);
    assert!(render.compositor.dirty);
}

#[test]
fn dirty_render_state_without_current_frame_is_not_presentable() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );

    render.mark_dirty();

    assert!(render.compositor.dirty);
    assert!(
        !render.has_presentable_dirty_content(),
        "redraw scheduling must wait until a glyph frame exists"
    );
}

#[test]
fn dirty_render_state_with_current_frame_is_presentable() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );

    render.set_current_frame(Some(make_frame(0x42, 0)), None, Default::default());
    render.mark_dirty();

    assert!(render.has_presentable_dirty_content());
}

#[test]
fn beginning_presentable_render_consumes_dirty_content() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );

    render.set_current_frame(Some(make_frame(0x42, 0)), None, Default::default());
    render.mark_dirty();

    render.begin_presentable_render();

    assert!(!render.compositor.dirty);
    assert!(!render.has_presentable_dirty_content());

    render.mark_dirty();

    assert!(render.has_presentable_dirty_content());
}

#[test]
fn frame_render_state_applies_visual_cursor_animation_rects() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let mut frame = make_frame(0x42, 0);
    frame.window_cursors.push(WindowCursor {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(-7),
        slot_id: DisplaySlotId::ZERO,
        x: 1.0,
        y: 2.0,
        width: 3.0,
        height: 4.0,
        style: CursorStyle::Hollow,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
        ascent: 0.0,
        active: false,
    });
    render.compositor.current_frame = Some(frame);
    let mut visual = crate::render_thread::cursor::CursorState::new(
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    visual.set_target(
        CursorTarget {
            window_id: -7,
            x: 11.0,
            y: 12.0,
            width: 13.0,
            height: 14.0,
            style: CursorStyle::Hollow,
            frame_id: 0x42,
        },
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.compositor.visual_cursors.insert(-7, visual);

    render.apply_visual_cursor_animations();

    let cursor = &render
        .compositor
        .current_frame
        .as_ref()
        .unwrap()
        .window_cursors[0];
    assert_eq!(cursor.x, 11.0);
    assert_eq!(cursor.y, 12.0);
    assert_eq!(cursor.width, 13.0);
    assert_eq!(cursor.height, 14.0);
}

#[test]
fn frame_render_state_drains_runtime_hints_once_for_render_clone() {
    let mut frame = make_frame(0x42, 0);
    let region = PresentedWindowRegions {
        text_body: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 80.0, 80.0),
        ..PresentedWindowRegions::default()
    }
    .buffer_viewport()
    .unwrap();
    frame.add_transition_hint(ContentTransitionHint::BufferReplaced {
        target: BufferTransitionTarget::Window {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(7),
            region,
        },
        intent: neomacs_display_protocol::ContentTransitionIntent::Replace,
    });
    frame.add_effect_hint(WindowEffectHint::WindowSwitchFade {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(7),
        bounds: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 80.0, 80.0),
    });

    let first = GuiFrameRenderState::take_frame_for_render(&mut frame);
    assert_eq!(first.transition_hints.len(), 1);
    assert_eq!(first.effect_hints.len(), 1);

    let second = GuiFrameRenderState::take_frame_for_render(&mut frame);
    assert!(second.transition_hints.is_empty());
    assert!(second.effect_hints.is_empty());
    assert!(frame.transition_hints.is_empty());
    assert!(frame.effect_hints.is_empty());
}

#[test]
fn frame_render_state_remove_child_frame_marks_dirty_when_removed() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(Some(make_frame(0x42, 0)), None, Default::default());
    let mut child = make_frame(0x99, 0x42);
    set_parent_offset(&mut child, 10.0, 20.0);
    render.compositor.child_frames.update_frame(child);

    assert!(render.remove_child_frame(0x99));

    assert!(render.compositor.child_frames.frames.is_empty());
    assert!(render.compositor.dirty);
}

#[test]
fn displayed_presentations_include_root_and_children_for_atomic_retirement() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let mut root = make_frame(0x42, 0);
    root.presentation_id = neomacs_display_protocol::frame_chrome::PresentationId::new(7);
    render.set_current_frame(Some(root), None, Default::default());
    let mut child = make_frame(0x99, 0x42);
    child.presentation_id = neomacs_display_protocol::frame_chrome::PresentationId::new(8);
    render.compositor.child_frames.update_frame(child);

    assert_eq!(
        render.displayed_presentations(),
        std::collections::HashSet::from([7, 8])
    );
    assert_eq!(render.child_presentation(0x99), Some(8));
}

#[test]
fn presented_pointer_hit_selects_the_displayed_root_or_child_map_in_local_coordinates() {
    use neomacs_display_protocol::{
        InteractionId, PointerAppearanceId, frame_chrome::PresentationId,
    };

    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let mut root = make_frame(0x42, 0);
    root.presentation_id = PresentationId::new(21);
    install_pointer_region(&mut root, None, true);
    render.set_current_frame(Some(root), None, Default::default());

    let mut child = make_frame(0x99, 0x42);
    child.presentation_id = PresentationId::new(22);
    set_parent_offset(&mut child, 100.0, 80.0);
    install_pointer_region(&mut child, Some(InteractionId::new(5)), false);
    assert!(render.update_child_frame(child));

    let root_hit = render
        .presented_pointer_hit(0x42, 5.0, 5.0)
        .unwrap()
        .expect("root visual-only hit");
    assert_eq!(root_hit.presentation(), PresentationId::new(21));
    assert_eq!(root_hit.interaction(), None);
    assert_eq!(
        root_hit.appearance_key().map(|key| key.appearance()),
        Some(PointerAppearanceId::try_from(0usize).unwrap())
    );

    let child_hit = render
        .presented_pointer_hit(0x99, 5.0, 5.0)
        .unwrap()
        .expect("child click-only local hit");
    assert_eq!(child_hit.presentation(), PresentationId::new(22));
    assert_eq!(child_hit.interaction(), Some(InteractionId::new(5)));
    assert_eq!(child_hit.appearance_key(), None);
    assert!(
        render
            .presented_pointer_hit(0x99, 105.0, 85.0)
            .unwrap()
            .is_none()
    );
}

#[test]
fn replacing_a_frame_clears_pointer_appearance_from_the_retired_presentation() {
    use crate::render_thread::state::{
        PresentedAppearanceKey, PresentedInteractionKey, PresentedPressCapture,
    };
    use neomacs_display_protocol::{
        InteractionId, PointerAppearanceId, frame_chrome::PresentationId,
    };

    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let mut old = make_frame(0x42, 0);
    old.presentation_id = PresentationId::new(7);
    render.set_current_frame(Some(old), None, Default::default());
    render
        .pointer_appearance
        .hover(Some(PresentedAppearanceKey::new(
            PresentationId::new(7),
            PointerAppearanceId::try_from(2usize).unwrap(),
        )));
    render.pointer_appearance.press();
    let captured = PresentedInteractionKey::new(PresentationId::new(7), InteractionId::new(42));
    render.capture_presented(Some(captured));

    let mut replacement = make_frame(0x42, 0);
    replacement.presentation_id = PresentationId::new(8);
    render.set_current_frame(Some(replacement), None, Default::default());

    assert_eq!(render.pointer_appearance.active(), None);
    assert_eq!(render.pointer_appearance.pressed(), None);
    assert_eq!(
        render.presented_capture(),
        Some(PresentedPressCapture::new(Some(captured))),
        "frame replacement retires visual state but keeps input capture until release"
    );
}

#[test]
fn replacing_or_removing_a_child_clears_only_its_pointer_appearance() {
    use crate::render_thread::state::PresentedAppearanceKey;
    use neomacs_display_protocol::{PointerAppearanceId, frame_chrome::PresentationId};

    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let mut root = make_frame(0x42, 0);
    root.presentation_id = PresentationId::new(7);
    render.set_current_frame(Some(root), None, Default::default());
    let mut child = make_frame(0x99, 0x42);
    child.presentation_id = PresentationId::new(17);
    assert!(render.update_child_frame(child));
    render
        .pointer_appearance
        .hover(Some(PresentedAppearanceKey::new(
            PresentationId::new(17),
            PointerAppearanceId::try_from(1usize).unwrap(),
        )));

    let mut root_replacement = make_frame(0x42, 0);
    root_replacement.presentation_id = PresentationId::new(8);
    render.set_current_frame(Some(root_replacement), None, Default::default());
    assert_eq!(
        render.pointer_appearance.active().unwrap().presentation(),
        PresentationId::new(17),
        "retiring the root must not clear a child presentation's appearance"
    );

    let mut replacement = make_frame(0x99, 0x42);
    replacement.presentation_id = PresentationId::new(18);
    assert!(render.update_child_frame(replacement));
    assert_eq!(render.pointer_appearance.active(), None);

    render
        .pointer_appearance
        .hover(Some(PresentedAppearanceKey::new(
            PresentationId::new(18),
            PointerAppearanceId::try_from(1usize).unwrap(),
        )));
    assert!(render.remove_child_frame(0x99));
    assert_eq!(render.pointer_appearance.active(), None);
}

#[test]
fn runtime_semantic_hit_query_uses_target_frame_presentation_and_rejects_stale_ids() {
    use neomacs_display_protocol::{
        DisplayWindowId, FrameRect, PresentedHitError, PresentedHitIndex, PresentedHitRegion,
        PresentedRegionKind, PresentedTextPosition, frame_chrome::PresentationId,
    };

    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    let presentation = PresentationId::new(7);
    let window = DisplayWindowId::new(9);
    let mut root = make_frame(0x42, 0);
    root.presentation_id = presentation;
    root.install_presented_hit_index(
        PresentedHitIndex::from_parts(
            presentation,
            vec![PresentedHitRegion::new(
                Some(window),
                PresentedRegionKind::TextBody,
                FrameRect::new(10.0, 20.0, 80.0, 40.0).unwrap(),
                0,
            )],
            vec![PresentedTextPosition::new(
                window,
                FrameRect::new(10.0, 20.0, 8.0, 16.0).unwrap(),
                55,
                0,
                0,
            )],
        )
        .unwrap(),
    )
    .unwrap();
    render.set_current_frame(Some(root), None, Default::default());

    let hit = render
        .presented_region_hit(0x42, presentation, 12.0, 22.0)
        .unwrap()
        .unwrap();
    assert_eq!(hit.region().kind(), PresentedRegionKind::TextBody);
    assert_eq!(hit.text_position().unwrap().buffer_position(), 55);
    assert_eq!(
        render.presented_region_hit(0x42, PresentationId::new(6), 12.0, 22.0),
        Err(PresentedHitError::StalePresentation {
            expected: presentation,
            requested: PresentationId::new(6),
        })
    );
}

#[test]
fn frame_render_state_remove_child_frame_ignores_late_stale_update() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(Some(make_frame(0x42, 0)), None, Default::default());
    let mut child = make_frame(0x99, 0x42);
    set_parent_offset(&mut child, 10.0, 20.0);

    render.update_child_frame(child.clone());
    assert!(render.compositor.child_frames.frames.contains_key(&0x99));

    assert!(render.remove_child_frame(0x99));
    assert!(render.compositor.child_frames.frames.is_empty());

    render.update_child_frame(child);

    assert!(
        render.compositor.child_frames.frames.is_empty(),
        "a child frame buffer queued before explicit removal must not re-add the hidden overlay"
    );
}

#[test]
fn frame_render_state_show_child_frame_allows_fresh_update_after_removal() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(Some(make_frame(0x42, 0)), None, Default::default());
    let mut child = make_frame(0x99, 0x42);
    set_parent_offset(&mut child, 10.0, 20.0);

    render.update_child_frame(child.clone());
    assert!(render.remove_child_frame(0x99));
    render.show_child_frame(0x99);
    render.update_child_frame(child);

    assert!(
        render.compositor.child_frames.frames.contains_key(&0x99),
        "a fresh child frame update after explicit visibility restore must be accepted"
    );
}

#[test]
fn frame_render_state_ignores_identical_child_frame_update() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(Some(make_frame(0x42, 0)), None, Default::default());
    let mut child = make_frame(0x99, 0x42);
    set_parent_offset(&mut child, 10.0, 20.0);

    assert!(render.update_child_frame(child.clone()));
    render.set_dirty(false);

    assert!(
        !render.update_child_frame(child),
        "an identical child frame packet should not dirty the compositor"
    );
    assert!(!render.compositor.dirty);
}

#[test]
fn frame_render_state_remove_child_cursor_clears_preedit() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.cursor.set_target(
        CursorTarget {
            window_id: 7,
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
            style: CursorStyle::Bar(1.0),
            frame_id: 0x99,
        },
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_ime_preedit("preedit".to_string(), Some((7, 7)));

    assert!(render.remove_child_frame(0x99));

    assert!(render.cursor.target_cloned().is_none());
    assert!(render.input_method.preedit().is_none());
    assert!(render.compositor.dirty);
}

#[test]
fn frame_render_state_preedit_update_replaces_composition_and_preserves_cursor() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );

    render.set_ime_preedit("ni".to_string(), Some((2, 2)));
    render.set_ime_preedit("你".to_string(), Some((3, 3)));

    let preedit = render
        .input_method
        .preedit()
        .expect("a non-empty preedit update must be active");
    assert_eq!(preedit.text, "你");
    assert_eq!(preedit.cursor_range, Some((3, 3)));

    render.set_ime_preedit(String::new(), None);
    assert!(
        render.input_method.preedit().is_none(),
        "winit defines an empty preedit update as clearing the composition"
    );
}

// =======================================================================
// GuiFrameWindowManager::new() — initial state
// =======================================================================

#[test]
fn new_manager_is_empty() {
    let mgr = GuiFrameWindowManager::new();
    assert!(mgr.windows.is_empty());
    assert!(mgr.winit_to_emacs.is_empty());
    assert!(mgr.pending_creates.is_empty());
    assert!(mgr.pending_destroys.is_empty());
}

#[test]
fn new_manager_count_is_zero() {
    let mgr = GuiFrameWindowManager::new();
    assert_eq!(mgr.count(), 0);
}

#[test]
fn clear_primary_mapping_removes_adopted_primary_identity() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.adopt_primary_frame_id(0x1000);
    mgr.clear_primary_mapping();

    assert_eq!(mgr.primary_frame_id(), None);
    assert_eq!(mgr.primary_event_frame_id(), 0);
    assert_eq!(mgr.primary_winit_id, None);
    assert!(mgr.winit_to_emacs.is_empty());
}

// =======================================================================
// request_create() — pending create queue
// =======================================================================

#[test]
fn request_create_adds_to_pending() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_create(
        1,
        800,
        600,
        "Test Window".to_string(),
        default_geometry_hints(),
    );

    assert_eq!(mgr.pending_creates.len(), 1);
    assert_eq!(mgr.pending_creates[0].emacs_frame_id, 1);
    assert_eq!(mgr.pending_creates[0].width, 800);
    assert_eq!(mgr.pending_creates[0].height, 600);
    assert_eq!(mgr.pending_creates[0].title, "Test Window");
}

#[test]
fn request_create_multiple_preserves_order() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_create(
        1,
        800,
        600,
        "Window 1".to_string(),
        default_geometry_hints(),
    );
    mgr.request_create(
        2,
        1024,
        768,
        "Window 2".to_string(),
        default_geometry_hints(),
    );
    mgr.request_create(
        3,
        1920,
        1080,
        "Window 3".to_string(),
        default_geometry_hints(),
    );

    assert_eq!(mgr.pending_creates.len(), 3);
    assert_eq!(mgr.pending_creates[0].emacs_frame_id, 1);
    assert_eq!(mgr.pending_creates[1].emacs_frame_id, 2);
    assert_eq!(mgr.pending_creates[2].emacs_frame_id, 3);
}

#[test]
fn request_create_does_not_modify_windows_map() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_create(1, 800, 600, "Test".to_string(), default_geometry_hints());

    // The window should NOT be in the windows map yet —
    // only in the pending queue until process_creates runs
    assert!(mgr.windows.is_empty());
    assert_eq!(mgr.count(), 0);
}

#[test]
fn request_create_allows_duplicate_frame_ids() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_create(1, 800, 600, "First".to_string(), default_geometry_hints());
    mgr.request_create(
        1,
        1024,
        768,
        "Duplicate".to_string(),
        default_geometry_hints(),
    );

    // Both are queued (process_creates will skip duplicates)
    assert_eq!(mgr.pending_creates.len(), 2);
}

#[test]
fn request_create_zero_dimensions() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_create(1, 0, 0, "Zero".to_string(), default_geometry_hints());

    assert_eq!(mgr.pending_creates.len(), 1);
    assert_eq!(mgr.pending_creates[0].width, 0);
    assert_eq!(mgr.pending_creates[0].height, 0);
}

#[test]
fn request_create_empty_title() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_create(1, 800, 600, String::new(), default_geometry_hints());

    assert_eq!(mgr.pending_creates[0].title, "");
}

#[test]
fn request_create_large_frame_id() {
    let mut mgr = GuiFrameWindowManager::new();
    let large_id = u64::MAX;
    mgr.request_create(
        large_id,
        800,
        600,
        "Max ID".to_string(),
        default_geometry_hints(),
    );

    assert_eq!(mgr.pending_creates[0].emacs_frame_id, large_id);
}

// =======================================================================
// request_destroy() — pending destroy queue
// =======================================================================

#[test]
fn request_destroy_adds_to_pending() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_destroy(42);

    assert_eq!(mgr.pending_destroys.len(), 1);
    assert_eq!(mgr.pending_destroys[0], 42);
}

#[test]
fn request_destroy_multiple_preserves_order() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_destroy(1);
    mgr.request_destroy(2);
    mgr.request_destroy(3);

    assert_eq!(mgr.pending_destroys.len(), 3);
    assert_eq!(mgr.pending_destroys, vec![1, 2, 3]);
}

#[test]
fn request_destroy_does_not_modify_windows_map() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_destroy(99);

    // Nothing should change in the actual windows map
    assert!(mgr.windows.is_empty());
    assert_eq!(mgr.count(), 0);
}

#[test]
fn request_destroy_nonexistent_frame_id_is_accepted() {
    let mut mgr = GuiFrameWindowManager::new();
    // No windows exist, but we can still queue a destroy
    mgr.request_destroy(999);
    assert_eq!(mgr.pending_destroys.len(), 1);
}

#[test]
fn request_destroy_duplicate_frame_ids_are_queued() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_destroy(1);
    mgr.request_destroy(1);

    assert_eq!(mgr.pending_destroys.len(), 2);
    assert_eq!(mgr.pending_destroys[0], 1);
    assert_eq!(mgr.pending_destroys[1], 1);
}

// =======================================================================
// process_destroys() — drain pending destroy queue
// =======================================================================

#[test]
fn process_destroys_drains_pending_queue() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_destroy(1);
    mgr.request_destroy(2);

    mgr.process_destroys();

    assert!(mgr.pending_destroys.is_empty());
}

#[test]
fn process_destroys_on_empty_queue_is_noop() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.process_destroys();
    assert!(mgr.pending_destroys.is_empty());
    assert!(mgr.windows.is_empty());
}

#[test]
fn process_destroys_nonexistent_frame_ids_does_not_panic() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_destroy(999);
    mgr.request_destroy(1000);

    // Should not panic even though these frame IDs don't exist in windows
    mgr.process_destroys();

    assert!(mgr.pending_destroys.is_empty());
}

// =======================================================================
// get() / get_mut() — lookup by emacs frame_id (empty manager)
// =======================================================================

#[test]
fn get_returns_none_for_empty_manager() {
    let mgr = GuiFrameWindowManager::new();
    assert!(mgr.get(1).is_none());
    assert!(mgr.get(0).is_none());
    assert!(mgr.get(u64::MAX).is_none());
}

#[test]
fn get_mut_returns_none_for_empty_manager() {
    let mut mgr = GuiFrameWindowManager::new();
    assert!(mgr.get_mut(1).is_none());
}

// =======================================================================
// emacs_frame_for_winit() — reverse lookup (empty manager)
// =======================================================================

// Note: We cannot construct winit::window::WindowId in tests
// (it's opaque), so we can only test the empty-map case indirectly
// by verifying the map is empty.

#[test]
fn winit_to_emacs_map_is_empty_initially() {
    let mgr = GuiFrameWindowManager::new();
    assert!(mgr.winit_to_emacs.is_empty());
}

// =======================================================================
// PendingWindow struct
// =======================================================================

#[test]
fn pending_window_stores_all_fields() {
    let pw = PendingWindow {
        emacs_frame_id: 123,
        width: 1920,
        height: 1080,
        title: "My Emacs Frame".to_string(),
        geometry_hints: default_geometry_hints(),
    };

    assert_eq!(pw.emacs_frame_id, 123);
    assert_eq!(pw.width, 1920);
    assert_eq!(pw.height, 1080);
    assert_eq!(pw.title, "My Emacs Frame");
}

#[test]
fn pending_window_unicode_title() {
    let pw = PendingWindow {
        emacs_frame_id: 1,
        width: 800,
        height: 600,
        title: "Emacs \u{2014} \u{1F680} Neomacs".to_string(),
        geometry_hints: default_geometry_hints(),
    };

    assert!(pw.title.contains('\u{2014}')); // em dash
    assert!(pw.title.contains('\u{1F680}')); // rocket emoji
}

// =======================================================================
// Mixed create/destroy queue operations
// =======================================================================

#[test]
fn create_and_destroy_queues_are_independent() {
    let mut mgr = GuiFrameWindowManager::new();

    mgr.request_create(1, 800, 600, "Win1".to_string(), default_geometry_hints());
    mgr.request_create(2, 1024, 768, "Win2".to_string(), default_geometry_hints());
    mgr.request_destroy(3);
    mgr.request_destroy(4);

    assert_eq!(mgr.pending_creates.len(), 2);
    assert_eq!(mgr.pending_destroys.len(), 2);

    // Processing destroys should not affect creates
    mgr.process_destroys();
    assert!(mgr.pending_destroys.is_empty());
    assert_eq!(mgr.pending_creates.len(), 2);
}

#[test]
fn process_destroys_called_twice_is_safe() {
    let mut mgr = GuiFrameWindowManager::new();
    mgr.request_destroy(1);

    mgr.process_destroys();
    assert!(mgr.pending_destroys.is_empty());

    // Second call should be a no-op
    mgr.process_destroys();
    assert!(mgr.pending_destroys.is_empty());
}

// =======================================================================
// count() — empty manager
// =======================================================================

#[test]
fn count_on_empty_manager() {
    let mgr = GuiFrameWindowManager::new();
    assert_eq!(mgr.count(), 0);
}

// =======================================================================
// Queue draining semantics: request + process + request again
// =======================================================================

#[test]
fn destroy_queue_refill_after_process() {
    let mut mgr = GuiFrameWindowManager::new();

    mgr.request_destroy(1);
    mgr.request_destroy(2);
    mgr.process_destroys();
    assert!(mgr.pending_destroys.is_empty());

    // Queue new destroys after processing
    mgr.request_destroy(3);
    mgr.request_destroy(4);
    assert_eq!(mgr.pending_destroys.len(), 2);
    assert_eq!(mgr.pending_destroys[0], 3);
    assert_eq!(mgr.pending_destroys[1], 4);
}

// =======================================================================
// WindowState struct fields (verify field existence/types)
// =======================================================================

// Note: WindowState cannot be constructed in tests because it requires
// Arc<Window>, wgpu::Surface, and wgpu::SurfaceConfiguration.
// The following test just verifies the struct's field count and layout
// by testing that the manager maps are properly typed.

#[test]
fn windows_map_key_is_frame_key() {
    let mgr = GuiFrameWindowManager::new();
    assert!(mgr.windows.get(&FrameKey::Pending).is_none());
    assert!(mgr.windows.get(&FrameKey::Adopted(0x1000)).is_none());
}

// =======================================================================
// Stress: many pending operations
// =======================================================================

#[test]
fn many_pending_creates() {
    let mut mgr = GuiFrameWindowManager::new();
    for i in 0..1000 {
        mgr.request_create(
            i,
            800,
            600,
            format!("Window {}", i),
            default_geometry_hints(),
        );
    }
    assert_eq!(mgr.pending_creates.len(), 1000);
    assert_eq!(mgr.pending_creates[0].emacs_frame_id, 0);
    assert_eq!(mgr.pending_creates[999].emacs_frame_id, 999);
}

#[test]
fn many_pending_destroys_processed() {
    let mut mgr = GuiFrameWindowManager::new();
    for i in 0..1000 {
        mgr.request_destroy(i);
    }
    assert_eq!(mgr.pending_destroys.len(), 1000);

    mgr.process_destroys();
    assert!(mgr.pending_destroys.is_empty());
}

#[test]
fn cursor_blink_toggle_asks_for_a_cursor_frame_not_a_repaint() {
    // A blink changes only the cursor layer, so it must not raise the content
    // dirty flag: that would demand a full repaint every half second and keep
    // the composite fast path from ever engaging in an idle session.
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(Some(make_frame(0x42, 0)), None, Default::default());
    render.cursor.set_target(
        crate::render_thread::cursor::CursorTarget {
            window_id: 7,
            x: 10.0,
            y: 20.0,
            width: 8.0,
            height: 16.0,
            style: CursorStyle::Bar(2.0),
            frame_id: 0x42,
        },
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.cursor.blink_enabled = true;
    render.cursor.blink_interval = std::time::Duration::from_millis(1);
    render.begin_presentable_render();
    assert!(!render.has_presentable_dirty_content());

    let toggled = render.tick_cursor_blink(
        neomacs_display_protocol::frame_time::observe_platform_now()
            .plus(std::time::Duration::from_millis(500)),
        false,
        None,
    );

    assert!(toggled, "the blink interval elapsed, so the cursor toggled");
    assert!(
        render.has_presentable_cursor_change(),
        "a blink toggle owes a cursor frame"
    );
    assert!(
        !render.has_presentable_dirty_content(),
        "a blink toggle must not demand a content repaint"
    );

    // Starting any frame satisfies both channels: every render path draws the
    // cursor at its current blink state.
    render.begin_presentable_render();
    assert!(!render.has_presentable_cursor_change());
}

#[test]
fn content_change_outranks_a_pending_cursor_change() {
    let Some(device) = make_test_device() else {
        return;
    };
    let mut render = GuiFrameRenderState::new(
        0x42,
        &device,
        1.0,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.set_current_frame(Some(make_frame(0x42, 0)), None, Default::default());
    render.compositor.cursor_dirty = true;
    render.mark_dirty();

    assert!(render.has_presentable_dirty_content());
    assert!(render.has_presentable_cursor_change());
    // The demand model merges strongest-invalidation-wins, so the repaint the
    // content owes covers the cursor layer too.
    render.begin_presentable_render();
    assert!(!render.has_presentable_dirty_content());
    assert!(!render.has_presentable_cursor_change());
}

// =======================================================================
// Observation state: creation is not an event that already happened
// =======================================================================

#[test]
fn a_new_window_has_never_had_its_titlebar_clicked() {
    // Seeding this with the creation time made the first click within the
    // double-click interval of opening a window maximize it instead of
    // starting a drag. "Never clicked" is a distinct state, so it gets one.
    let chrome = crate::render_thread::state::WindowChrome::default();
    assert!(chrome.last_titlebar_click.is_none());
}

#[test]
fn a_new_window_counts_its_creation_as_activity_so_it_does_not_dim_at_once() {
    // IdleDimState deliberately has no Default: there is no honest zero value
    // for "when was this window last active", and treating None as "idle
    // forever" would dim a freshly-opened window immediately.
    let created = neomacs_display_protocol::frame_time::observe_platform_now();
    let idle = crate::render_thread::state::IdleDimState::new(created);
    assert_eq!(idle.last_activity_time, created);
    assert_eq!(idle.current_alpha, 0.0);
    assert!(!idle.active);
    // No time has passed as of creation, so nothing is idle yet.
    assert_eq!(
        created.saturating_since(idle.last_activity_time),
        std::time::Duration::ZERO
    );
}
