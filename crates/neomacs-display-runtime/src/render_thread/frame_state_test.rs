use super::*;
use crate::core::face::Face;
use crate::core::frame_glyphs::{CursorStyle, FrameGlyphBuffer, GlyphRowRole, PhysCursor};
use crate::thread_comm::ThreadComms;
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::{Color, DisplayWindowId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn make_test_app() -> RenderApp {
    let comms = ThreadComms::new();
    let (_emacs, render) = comms.split();
    RenderApp::new(
        render,
        800,
        600,
        "test".to_string(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new())),
        true,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    )
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
        label: Some("frame-state test device"),
        ..Default::default()
    }))
    .ok()?;
    Some(device)
}

fn face(id: FaceId) -> Face {
    Face {
        id,
        ..Face::default()
    }
}

fn named_face(id: FaceId, name: &str) -> Face {
    Face {
        id,
        lisp_name: Some(name.to_owned()),
        ..Face::default()
    }
}

#[test]
fn face_change_summary_distinguishes_added_modified_and_removed_faces() {
    let unchanged_id = FaceId::new(1);
    let modified_id = FaceId::new(2);
    let removed_id = FaceId::new(3);
    let added_id = FaceId::new(4);
    let old = HashMap::from([
        (unchanged_id, named_face(unchanged_id, "unchanged")),
        (modified_id, named_face(modified_id, "modified")),
        (removed_id, named_face(removed_id, "removed")),
    ]);
    let mut modified = named_face(modified_id, "modified");
    modified.font_size = 18.0;
    let new = HashMap::from([
        (unchanged_id, named_face(unchanged_id, "unchanged")),
        (modified_id, modified),
        (added_id, named_face(added_id, "added")),
    ]);

    let summary = summarize_face_changes(&old, &new);

    assert_eq!(
        summary,
        FaceChangeSummary {
            added: 1,
            modified: 1,
            raster_modified: 1,
            removed: 1,
        }
    );
}

#[test]
fn recoloring_faces_is_not_classified_as_raster_relevant() {
    // Typing recolors anonymous font-lock realizations every keystroke.
    // Coverage masks are color-independent (color is applied at draw time),
    // so a foreground/background-only change must not count as
    // raster-relevant in diagnostics.
    let id = FaceId::new(1);
    let old = HashMap::from([(id, named_face(id, "recolored"))]);
    let mut recolored = named_face(id, "recolored");
    recolored.foreground = Color::new(0.9, 0.1, 0.1, 1.0);
    let new = HashMap::from([(id, recolored)]);

    let summary = summarize_face_changes(&old, &new);

    assert_eq!(summary.modified, 1);
    assert_eq!(summary.raster_modified, 0);
}

#[test]
fn adding_faces_is_classified_as_added_not_modified() {
    // An added face cannot collide with existing content-addressed entries:
    // its glyph keys either share a font identity (correct by definition)
    // or allocate fresh entries.
    let retained_id = FaceId::new(1);
    let added_id = FaceId::new(2);
    let old = HashMap::from([(retained_id, face(retained_id))]);
    let new = HashMap::from([
        (retained_id, face(retained_id)),
        (added_id, named_face(added_id, "fringe")),
    ]);

    let summary = summarize_face_changes(&old, &new);

    assert_eq!(summary.added, 1);
    assert_eq!(summary.modified, 0);
}

#[test]
fn changing_a_face_font_is_classified_as_raster_relevant() {
    let id = FaceId::new(1);
    let old = HashMap::from([(id, named_face(id, "refonted"))]);
    let mut refonted = named_face(id, "refonted");
    refonted.font_family = "Iosevka".to_owned();
    let new = HashMap::from([(id, refonted)]);

    let summary = summarize_face_changes(&old, &new);

    assert_eq!(summary.raster_modified, 1);
}

#[test]
fn removing_faces_alone_is_classified_as_removed() {
    let retained_id = FaceId::new(1);
    let removed_id = FaceId::new(2);
    let old = HashMap::from([
        (retained_id, face(retained_id)),
        (removed_id, face(removed_id)),
    ]);
    let new = HashMap::from([(retained_id, face(retained_id))]);

    let summary = summarize_face_changes(&old, &new);

    assert_eq!(summary.removed, 1);
    assert_eq!(summary.modified, 0);
}

#[test]
fn face_diff_details_are_sorted_bounded_and_name_changed_fields() {
    let old = HashMap::from([
        (FaceId::new(2), named_face(FaceId::new(2), "second")),
        (FaceId::new(1), named_face(FaceId::new(1), "first")),
    ]);
    let mut first = named_face(FaceId::new(1), "first");
    first.font_size = 12.0;
    let mut second = named_face(FaceId::new(2), "second");
    second.font_weight = 700;
    let new = HashMap::from([(FaceId::new(2), second), (FaceId::new(1), first)]);

    let details = build_face_diff_details(&old, &new, 1);

    assert_eq!(details.modified.len(), 1);
    assert_eq!(details.modified[0].id, FaceId::new(1));
    assert!(
        details.modified[0]
            .fields
            .iter()
            .any(|field| field.starts_with("font_size="))
    );
    assert_eq!(details.modified_omitted, 1);
}

#[test]
fn changed_face_sources_include_ingest_sequence_updates() {
    let old = vec![(1, 10), (2, 20)];
    let new = vec![(1, 11), (3, 30)];

    let details = changed_face_sources(&old, &new, 2);

    assert_eq!(details.changes.len(), 2);
    assert_eq!(details.omitted, 1);
    assert_eq!(details.changes[0].frame_id, 1);
    assert_eq!(details.changes[0].old_ingest_sequences, vec![10]);
    assert_eq!(details.changes[0].new_ingest_sequences, vec![11]);
    assert_eq!(details.changes[1].frame_id, 2);
    assert!(details.changes[1].new_ingest_sequences.is_empty());
}

#[test]
fn face_id_conflicts_are_sorted_and_bounded() {
    let face_id = FaceId::new(7);
    let baseline = named_face(face_id, "baseline");
    let mut conflicting = named_face(face_id, "conflicting");
    conflicting.font_size = 18.0;
    let occurrence = |frame_id, face: Face| FaceOccurrence {
        face_id,
        frame_id,
        sort_key: serde_json::to_string(&face).expect("serialize face"),
        face,
    };

    let details = build_face_conflict_details(
        vec![
            occurrence(30, conflicting.clone()),
            occurrence(20, conflicting),
            occurrence(10, baseline),
        ],
        1,
    );

    assert_eq!(details.conflicts.len(), 1);
    assert_eq!(details.omitted, 1);
    assert_eq!(details.conflicts[0].first_frame_id, 10);
    assert_eq!(details.conflicts[0].conflicting_frame_id, 20);
}

#[test]
fn apply_extra_spacing_remaps_cursor_by_slot_id() {
    let mut frame = FrameGlyphBuffer::with_size(80.0, 32.0);
    frame.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    frame.add_char('a', 0.0, 0.0, 8.0, 16.0, 12.0, false);
    frame.add_char('b', 8.0, 0.0, 8.0, 16.0, 12.0, false);
    let target_slot = frame.glyphs[1].slot_id().expect("slot id");

    frame.add_cursor(
        DisplayWindowId::new(1),
        2.0,
        0.0,
        2.0,
        16.0,
        CursorStyle::Bar(2.0),
        Color::WHITE,
    );
    frame.window_cursors[0].slot_id = target_slot;

    frame.set_phys_cursor(PhysCursor {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(1),
        charpos: 1,
        row: 0,
        col: 1,
        slot_id: target_slot,
        x: 2.0,
        y: 0.0,
        width: 2.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::Bar(2.0),
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
    });

    RenderApp::apply_extra_spacing(&mut frame.glyphs, &mut frame.window_cursors, 0.0, 1.0);

    match &frame.glyphs[1] {
        FrameGlyph::Char { x, .. } => assert_eq!(*x, 9.0),
        other => panic!("expected char glyph, got {:?}", other),
    }
    assert_eq!(frame.window_cursors[0].x, 9.0);
    assert_eq!(frame.window_cursors[0].y, 0.0);
    let cursor = frame.active_cursor().expect("active cursor");
    assert_eq!(cursor.x, 9.0);
    assert_eq!(cursor.y, 0.0);
}

#[test]
fn refresh_faces_rebuilds_from_primary_fallback_frames() {
    let mut app = make_test_app();
    let Some(device) = make_test_device() else {
        return;
    };
    let __render = super::super::frame_windows::GuiFrameRenderState::new(
        0,
        &device,
        app.frame_windows
            .primary_window()
            .map_or(1.0, |ws| ws.scale_factor()),
        app.frame_windows.fps_enabled,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    if let Some(window_state) = app.frame_windows.primary_window_mut() {
        window_state.render = __render;
    }
    app.faces.insert(FaceId::new(99), face(FaceId::new(99)));

    let mut root = FrameGlyphBuffer::with_size(80.0, 32.0);
    root.faces.insert(FaceId::new(7), face(FaceId::new(7)));
    if let Some(ws) = app.frame_windows.primary_window_mut() {
        ws.render.set_current_frame(Some(root), None);
    };

    let mut child = FrameGlyphBuffer::with_size(40.0, 16.0);
    child.set_frame_identity(
        neomacs_display_protocol::types::DisplayFrameId::new(0x2000),
        neomacs_display_protocol::types::DisplayFrameId::new(0),
        0.0,
        0.0,
        0,
        false,
        0.0,
        Color::BLACK,
        false,
        1.0,
    );
    child.faces.insert(FaceId::new(8), face(FaceId::new(8)));
    let mut conflicting_face = named_face(FaceId::new(7), "child-conflict");
    conflicting_face.font_size = 18.0;
    child.faces.insert(FaceId::new(7), conflicting_face);
    app.frame_windows
        .primary_window_mut()
        .expect("primary child frames mut")
        .render
        .compositor
        .child_frames
        .update_frame(child);

    app.refresh_faces_from_frames();

    assert!(app.faces.contains_key(&FaceId::new(7)));
    assert!(app.faces.contains_key(&FaceId::new(8)));
    assert!(!app.faces.contains_key(&FaceId::new(99)));

    // An unchanged frame signature must skip the rebuild entirely.
    app.refresh_faces_from_frames();
    assert!(app.faces.contains_key(&FaceId::new(7)));

    let conflicts = app.collect_face_id_conflicts(10);
    assert_eq!(conflicts.conflicts.len(), 1);
    assert_eq!(conflicts.conflicts[0].face_id, FaceId::new(7));
    assert_eq!(conflicts.conflicts[0].conflicting_frame_id, 0x2000);
}

// =======================================================================
// FPS counter: two clocks, deliberately separated
// =======================================================================

#[test]
fn displayed_fps_is_exactly_frames_over_the_sample_span() {
    use neomacs_display_protocol::frame_time::{FrameSample, observe_platform_now};

    // This assertion is exact, which was impossible while the counter read the
    // wall clock: 60 frames spaced one 60Hz interval apart must read 60 fps.
    let interval = std::time::Duration::from_secs_f64(1.0 / 60.0);
    let start = observe_platform_now();
    let mut fps = crate::render_thread::state::FpsCounter {
        enabled: true,
        last_instant: start,
        ..Default::default()
    };
    fps.enabled = true;

    for frame in 1..=60u32 {
        let sample = FrameSample::new(start.plus(interval * frame), interval);
        RenderApp::update_fps_counter(&mut fps, sample);
    }

    assert!(
        (fps.display_value - 60.0).abs() < 1e-3,
        "expected exactly 60 fps, got {}",
        fps.display_value
    );
    assert_eq!(fps.frame_count, 0, "the window resets once it reports");
}

#[test]
fn a_disabled_fps_counter_does_not_advance() {
    use neomacs_display_protocol::frame_time::{FrameSample, observe_platform_now};

    let start = observe_platform_now();
    let mut fps = crate::render_thread::state::FpsCounter {
        enabled: false,
        last_instant: start,
        ..Default::default()
    };
    let sample = FrameSample::new(
        start.plus(std::time::Duration::from_secs(2)),
        interval_16ms(),
    );
    RenderApp::update_fps_counter(&mut fps, sample);
    assert_eq!(fps.frame_count, 0);
    assert_eq!(fps.display_value, 0.0);
}

fn interval_16ms() -> std::time::Duration {
    std::time::Duration::from_millis(16)
}
