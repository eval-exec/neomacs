//! Differential ownership tests for the terminal, fresh window-chrome handoff.
//! The borrowed reference uses the old materializer and Complete lifecycle.
use super::*;
use crate::display_item::DisplaySourcePosition;
use crate::display_row::builder::DisplayRowGlyphSlot;
use crate::display_row::measured_state::DisplayRowBoundsPolicy;
use crate::display_row::render_state::{DisplayRowOutputProgress, RenderedDisplayRow};
use crate::frame_face_arena::{FrameFaceArena, FrameFaceAttempt};
use crate::output::row_request::OutputRowLifecycleRequest;
use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{
    FringeBitmapInfo, Glyph, GlyphArea, GlyphPointerAppearance, GlyphPointerOccurrenceIdentity,
    GlyphPointerSourceIdentity, GlyphPointerSourceKind, GlyphProvenance, GlyphRow,
    GlyphStringBufferRange, GlyphStringId, GlyphStringSource, GlyphType,
};
use neomacs_display_protocol::types::{FaceId, Rect};
use neomacs_display_protocol::{ImageMargins, ImageOpaqueBackground, ImageSourceRect};
use neovm_core::buffer::{BufferId, CharPos0, EmacsBytePos};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn rich_rendered(role: GlyphRowRole, faces: Vec<RealizedFrameFace>) -> RenderedDisplayRow {
    let face = FaceId::new(101);
    let mut row = GlyphRow::new(role);
    row.enabled = true;
    row.hash = 0x1234_5678;
    row.mode_line = role == GlyphRowRole::ModeLine;
    row.cursor_col = Some(2);
    row.cursor_type = Some(neomacs_display_protocol::frame_glyphs::CursorStyle::Bar(
        2.0,
    ));
    row.truncated_left = true;
    row.continued = true;
    row.ends_at_zv = true;
    row.pixel_x = 3.0;
    row.start_col = 2;
    row.pixel_y = 43.0;
    row.height_px = 30.0;
    row.ascent_px = 20.0;
    row.line_spacing_px = 2.0;
    row.start_charpos = 999;
    row.end_charpos = 1000;
    row.left_fringe_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 4,
        face_id: FaceId::new(102),
    });
    row.right_fringe_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 5,
        face_id: face,
    });
    row.overlay_arrow_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 6,
        face_id: face,
    });
    let source = row
        .push_string_source(GlyphStringSource::replacement(
            GlyphStringId::new(0x1234),
            GlyphStringBufferRange::new(7, 12),
        ))
        .expect("string source");
    let pointer = row
        .intern_pointer_appearance(GlyphPointerAppearance {
            source: GlyphPointerSourceIdentity {
                kind: GlyphPointerSourceKind::LispString,
                source_id: 0x1234,
                range_start: 0,
                range_end: 3,
                property_owner: 0x5678,
                occurrence: GlyphPointerOccurrenceIdentity::BufferDisplayReplacement {
                    buffer_id: 9,
                    anchor: 7,
                },
            },
            face_id: face,
        })
        .expect("pointer appearance");
    let margins = row
        .intern_image_margins(ImageMargins::asymmetric(1.0, 2.0, 3.0, 4.0))
        .expect("image margins");
    row.glyphs[GlyphArea::LeftMargin.index()].push(Glyph::char('<', face, 7));
    row.glyphs[GlyphArea::RightMargin.index()].push(Glyph::char('>', face, 11));
    let mut cluster = Glyph::char_with_provenance('e', face, GlyphProvenance::string(source, 1))
        .with_pixel_geometry(16.0, 22.0, 17.0)
        .with_vertical_offset(-2.0);
    cluster.glyph_type = GlyphType::Composite {
        text: "e\u{301}".into(),
    };
    cluster.pointer_appearance = Some(pointer);
    cluster.box_vertical_edges = neomacs_display_protocol::face::BoxVerticalEdges::Both;
    row.glyphs[GlyphArea::Text.index()].push(cluster);
    let mut padding = Glyph::char(' ', face, 8).with_pixel_geometry(0.0, 22.0, 17.0);
    padding.padding = true;
    padding.wide = true;
    row.glyphs[GlyphArea::Text.index()].push(padding);
    let mut image = Glyph::stretch(3, face)
        .with_pixel_geometry(24.0, 28.0, 19.0)
        .with_vertical_offset(3.0);
    image.glyph_type = GlyphType::Image {
        image_id: 12,
        width_cols: 3,
        source_rect: ImageSourceRect::new(0.25, 0.0, 0.5, 1.0).expect("source rectangle"),
        margins,
        opaque_background: ImageOpaqueBackground::new(Some(0x12_34_56)),
    };
    row.glyphs[GlyphArea::Text.index()].push(image);
    let mut widget = Glyph::stretch(2, face).with_pixel_geometry(16.0, 24.0, 18.0);
    widget.glyph_type = GlyphType::Xwidget {
        xwidget_id: neomacs_display_protocol::XwidgetId::new(1234),
        webview_id: neomacs_display_protocol::WebViewId::new(5678),
        width_cols: 2,
        content: neomacs_display_protocol::XwidgetContentExtent::new(40.0, 36.0)
            .expect("widget content"),
    };
    row.glyphs[GlyphArea::Text.index()].push(widget);
    row.rebuild_pointer_runs(8.0, 160.0);
    assert!(!row.pointer_runs().is_empty());
    let slots = vec![
        DisplayRowGlyphSlot::new(
            DisplaySourcePosition::buffer(BufferId(9), CharPos0::new(7), EmacsBytePos::new(8)),
            0.0,
            0,
            16.0,
            2,
        ),
        DisplayRowGlyphSlot::new(
            DisplaySourcePosition::lisp_string(0x1234, 100, 101),
            16.0,
            2,
            0.0,
            0,
        ),
        DisplayRowGlyphSlot::new(
            DisplaySourcePosition::buffer(BufferId(9), CharPos0::new(11), EmacsBytePos::new(15)),
            16.0,
            2,
            24.0,
            3,
        ),
        DisplayRowGlyphSlot::new(DisplaySourcePosition::synthetic(77, 999), 40.0, 5, 16.0, 2),
    ];
    RenderedDisplayRow::new(
        row,
        DisplayRowOutputProgress::new(56.0, 7, 43.0, 30.0),
        slots,
        faces,
    )
}

fn prepared_faces(attempt: &FrameFaceAttempt) -> Vec<RealizedFrameFace> {
    [101, 102]
        .into_iter()
        .map(|id| {
            let mut face = Face::new(FaceId::new(id));
            face.font_family = std::sync::Arc::from(format!("move-test-{id}"));
            attempt.prepare_face(face).expect("unpublished face")
        })
        .collect()
}

fn measured(
    kind: WindowChromeKind,
    policy: DisplayRowBoundsPolicy,
    y: f32,
    faces: Vec<RealizedFrameFace>,
) -> MeasuredDisplayRow {
    let role = match kind {
        WindowChromeKind::TabLine => GlyphRowRole::TabLine,
        WindowChromeKind::HeaderLine => GlyphRowRole::HeaderLine,
        WindowChromeKind::ModeLine => GlyphRowRole::ModeLine,
    };
    MeasuredDisplayRow::new(
        DisplayRowOwner::WindowChrome {
            window_id: 77,
            kind,
        },
        1,
        Rect::new(13.0, y, 160.0, 54.0),
        rich_rendered(role, faces),
        policy,
    )
}

fn window_builder(attempt: FrameFaceAttempt) -> DisplayOutputBuilder {
    let mut builder = DisplayOutputBuilder::new();
    builder.set_face_attempt(attempt);
    builder.begin_window_with_text_bounds(
        77,
        3,
        20,
        Rect::new(13.0, 16.0, 180.0, 100.0),
        Rect::new(21.0, 40.0, 160.0, 64.0),
        true,
    );
    builder
}

// Literal old borrowed window handoff. In particular, this never calls the
// consuming installer, so a regression in its owner/geometry/lifecycle is visible.
fn install_borrowed_reference(builder: &mut DisplayOutputBuilder, measured: &MeasuredDisplayRow) {
    let DisplayRowOwner::WindowChrome { window_id, .. } = measured.owner() else {
        panic!("window fixture");
    };
    assert_eq!(window_id as i64, builder.current_window_id_i64());
    install_faces(builder, measured.rendered().faces());
    let row = measured.window_relative_output_row(builder.current_window_pixel_bounds());
    builder.install_output_row_lifecycle(OutputRowLifecycleRequest::complete(
        measured.row_index() as usize,
        row.role,
        row.mode_line,
        row,
    ));
}

#[test]
fn consuming_materializer_moves_storage_and_complete_payload_with_fresh_identity() {
    let rendered = rich_rendered(GlyphRowRole::ModeLine, Vec::new());
    let original = rendered.row();
    let appearance = original.appearance_id();
    let glyph_allocations = original
        .glyphs
        .each_ref()
        .map(|glyphs| (glyphs.as_ptr(), glyphs.capacity()));
    assert!(original.glyphs.iter().all(|glyphs| !glyphs.is_empty()));
    let pointer_allocation = original.pointer_appearances().as_ptr();
    let margin_allocation = original.image_margins_table().as_ptr();
    let source_allocation = original.string_sources().as_ptr();
    let run_allocation = original.pointer_runs().as_ptr();
    let composite_allocation = match &original.glyphs[GlyphArea::Text.index()][0].glyph_type {
        GlyphType::Composite { text } => text.as_ptr(),
        _ => panic!("cluster fixture"),
    };
    let reference = rendered.materialize_output_row(27.0, 31.0, 19.0);
    let other_live_row = GlyphRow::new(GlyphRowRole::ModeLine);
    assert_ne!(appearance, reference.appearance_id());
    assert_ne!(appearance, other_live_row.appearance_id());
    let moved = rendered.into_materialized_output_row(27.0, 31.0, 19.0);
    assert_eq!(moved, reference); // Includes every GlyphRow field and dependency table.
    assert_eq!(moved.appearance_id(), appearance);
    assert_eq!(
        moved
            .glyphs
            .each_ref()
            .map(|glyphs| (glyphs.as_ptr(), glyphs.capacity())),
        glyph_allocations
    );
    assert_eq!(moved.pointer_appearances().as_ptr(), pointer_allocation);
    assert_eq!(moved.image_margins_table().as_ptr(), margin_allocation);
    assert_eq!(moved.string_sources().as_ptr(), source_allocation);
    assert_eq!(moved.pointer_runs().as_ptr(), run_allocation);
    match &moved.glyphs[GlyphArea::Text.index()][0].glyph_type {
        GlyphType::Composite { text } => assert_eq!(text.as_ptr(), composite_allocation),
        _ => panic!("cluster transported"),
    }
    assert_eq!((moved.start_charpos, moved.end_charpos), (7, 12));
    assert_eq!(
        (moved.pixel_y, moved.height_px, moved.ascent_px),
        (27.0, 31.0, 19.0)
    );
    assert!(moved.displays_text);
}

#[test]
fn consuming_empty_disabled_row_preserves_metadata_and_normalizes_text_flag() {
    let mut row = GlyphRow::new(GlyphRowRole::HeaderLine);
    row.enabled = false;
    row.displays_text = true; // Deliberately stale: both materializers must normalize.
    row.hash = 57;
    row.glyphs[GlyphArea::LeftMargin.index()].push(Glyph::char('<', FaceId::new(101), 4));
    let rendered = RenderedDisplayRow::new(
        row,
        DisplayRowOutputProgress::new(0.0, 0, 0.0, 1.0),
        vec![],
        vec![],
    );
    let reference = rendered.materialize_output_row(-3.0, 1.0, 0.0);
    let moved = rendered.into_materialized_output_row(-3.0, 1.0, 0.0);
    assert_eq!(moved, reference);
    assert!(!moved.enabled);
    assert!(!moved.displays_text);
    assert_eq!(moved.hash, 57);
    assert_eq!(moved.glyphs[GlyphArea::LeftMargin.index()].len(), 1);
}

#[test]
fn consuming_measured_rows_match_borrowed_geometry_for_all_window_chrome_kinds() {
    for kind in [
        WindowChromeKind::TabLine,
        WindowChromeKind::HeaderLine,
        WindowChromeKind::ModeLine,
    ] {
        for policy in [
            DisplayRowBoundsPolicy::PreserveAllocatedMinimum,
            DisplayRowBoundsPolicy::MeasureIntrinsic,
            DisplayRowBoundsPolicy::MeasureContent,
        ] {
            for y in [-4.0, 40.0, 88.0] {
                let mut measured = measured(kind, policy, 0.0, vec![]);
                measured.reanchor_y(y);
                let window = Rect::new(13.0, 16.0, 180.0, 100.0);
                let reference = measured.window_relative_output_row(window);
                let progress = measured.output_progress();
                let appearance = measured.rendered().row().appearance_id();
                let moved = measured.into_window_relative_output_row(window);
                assert_eq!(moved, reference, "kind={kind:?} policy={policy:?} y={y}");
                assert_eq!(moved.appearance_id(), appearance);
                assert_eq!(moved.pixel_y, y - window.y);
                assert_eq!(progress.y(), y);
                assert_eq!(progress.height(), moved.height_px);
            }
        }
    }
}

#[test]
fn consuming_window_install_matches_complete_borrowed_output_and_publishes_all_faces() {
    for kind in [
        WindowChromeKind::TabLine,
        WindowChromeKind::HeaderLine,
        WindowChromeKind::ModeLine,
    ] {
        let arena = FrameFaceArena::default();
        let reference_attempt = arena.begin_attempt();
        let candidate_attempt = arena.begin_attempt();
        let reference = measured(
            kind,
            DisplayRowBoundsPolicy::MeasureIntrinsic,
            43.0,
            prepared_faces(&reference_attempt),
        );
        let candidate = measured(
            kind,
            DisplayRowBoundsPolicy::MeasureIntrinsic,
            43.0,
            prepared_faces(&candidate_attempt),
        );
        let source_appearance = candidate.rendered().row().appearance_id();
        assert!(reference_attempt.faces().is_empty());
        assert!(candidate_attempt.faces().is_empty());
        let mut expected = window_builder(reference_attempt);
        let mut actual = window_builder(candidate_attempt);
        install_borrowed_reference(&mut expected, &reference);
        install_measured_window_display_row(&mut actual, candidate);
        assert_eq!(actual.current_window_row(1), expected.current_window_row(1));
        assert_eq!(
            actual
                .current_window_row(1)
                .expect("installed row")
                .appearance_id(),
            source_appearance
        );
        for id in [101, 102] {
            assert_eq!(
                actual.output_face(FaceId::new(id)),
                expected.output_face(FaceId::new(id))
            );
            assert!(actual.output_face(FaceId::new(id)).is_some());
        }
        actual.end_window();
        expected.end_window();
        let actual = actual.finish(80, 24, 8.0, 16.0);
        let expected = expected.finish(80, 24, 8.0, 16.0);
        assert_eq!(
            serde_json::to_value(&actual).expect("actual frame"),
            serde_json::to_value(&expected).expect("reference frame")
        );
        assert_eq!(actual.materialize(), expected.materialize());
    }
}

#[test]
fn foreign_attempt_face_refuses_before_installing_a_window_row() {
    let arena = FrameFaceArena::default();
    let foreign = arena.begin_attempt();
    let own = arena.begin_attempt();
    let face = foreign
        .prepare_face(Face::new(FaceId::new(101)))
        .expect("foreign prepared face");
    let candidate = measured(
        WindowChromeKind::TabLine,
        DisplayRowBoundsPolicy::MeasureIntrinsic,
        43.0,
        vec![face],
    );
    let mut builder = window_builder(own.clone());
    let before = builder.current_window_row(1).cloned();
    let result = catch_unwind(AssertUnwindSafe(|| {
        install_measured_window_display_row(&mut builder, candidate)
    }));
    let payload = result.expect_err("foreign face must be refused");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        message.contains("published face must belong to the output attempt"),
        "{message}"
    );
    assert_eq!(builder.current_window_row(1), before.as_ref());
    assert!(own.faces().is_empty());
    assert!(foreign.faces().is_empty());
}

#[test]
fn frame_owner_refuses_window_consumption_before_face_publication() {
    let arena = FrameFaceArena::default();
    let attempt = arena.begin_attempt();
    let frame = MeasuredDisplayRow::new(
        DisplayRowOwner::FrameChrome {
            kind: FrameChromeKind::TabBar,
        },
        0,
        Rect::new(0.0, 0.0, 160.0, 30.0),
        rich_rendered(GlyphRowRole::TabBar, prepared_faces(&attempt)),
        DisplayRowBoundsPolicy::MeasureIntrinsic,
    );
    let mut builder = window_builder(attempt.clone());
    let before = builder.current_window_row(0).cloned();
    let result = catch_unwind(AssertUnwindSafe(|| {
        install_measured_window_display_row(&mut builder, frame)
    }));
    let payload = result.expect_err("frame owner cannot use terminal window handoff");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        message.contains("frame chrome rows must install through frame chrome rows"),
        "{message}"
    );
    assert_eq!(builder.current_window_row(0), before.as_ref());
    assert!(attempt.faces().is_empty());
}

#[test]
fn frame_asset_publication_keeps_the_borrowed_source_available_for_later_views() {
    let arena = FrameFaceArena::default();
    let attempt = arena.begin_attempt();
    let measured = MeasuredDisplayRow::new(
        DisplayRowOwner::FrameChrome {
            kind: FrameChromeKind::TabBar,
        },
        0,
        Rect::new(0.0, 40.0, 160.0, 30.0),
        rich_rendered(GlyphRowRole::TabBar, prepared_faces(&attempt)),
        DisplayRowBoundsPolicy::MeasureIntrinsic,
    );
    let appearance = measured.rendered().row().appearance_id();
    let original_storage = measured.rendered().row().glyphs[GlyphArea::Text.index()].as_ptr();
    let source_snapshot = measured.rendered().row().clone();
    let mut builder = DisplayOutputBuilder::new();
    builder.set_face_attempt(attempt.clone());
    assert!(attempt.faces().is_empty());
    install_measured_frame_chrome_display_row(&mut builder, &measured);
    assert!(builder.output_face(FaceId::new(101)).is_some());
    assert!(builder.output_face(FaceId::new(102)).is_some());
    // These reads occur after publication in the real frame-tabbar owner.
    let first = frame_chrome_display_row(&measured);
    let second = frame_chrome_display_row(&measured);
    assert_eq!(first.row(), second.row());
    let frame_local = measured.frame_chrome_output_row();
    let relative = measured.window_relative_output_row(Rect::new(0.0, 16.0, 160.0, 100.0));
    assert_eq!(frame_local.pixel_y, 0.0);
    assert_eq!(relative.pixel_y, measured.bounds().y - 16.0);
    assert_eq!(measured.rendered().row(), &source_snapshot);
    assert_eq!(measured.rendered().row().appearance_id(), appearance);
    assert_eq!(
        measured.rendered().row().glyphs[GlyphArea::Text.index()].as_ptr(),
        original_storage
    );
    assert!(!measured.rendered().source_slots().is_empty());
}

#[test]
fn rtl_window_complete_lifecycle_still_reorders_once_against_borrowed_reference() {
    fn rtl() -> MeasuredDisplayRow {
        let mut row = GlyphRow::new(GlyphRowRole::HeaderLine);
        row.enabled = true;
        row.height_px = 18.0;
        row.ascent_px = 13.0;
        row.glyphs[GlyphArea::Text.index()].push(Glyph::char('א', FaceId::new(101), 7));
        row.glyphs[GlyphArea::Text.index()].push(Glyph::char('ב', FaceId::new(101), 8));
        MeasuredDisplayRow::new(
            DisplayRowOwner::WindowChrome {
                window_id: 77,
                kind: WindowChromeKind::HeaderLine,
            },
            1,
            Rect::new(13.0, 40.0, 160.0, 18.0),
            RenderedDisplayRow::new(
                row,
                DisplayRowOutputProgress::new(16.0, 2, 40.0, 18.0),
                vec![],
                vec![],
            ),
            DisplayRowBoundsPolicy::MeasureIntrinsic,
        )
    }
    let arena = FrameFaceArena::default();
    let mut expected = window_builder(arena.begin_attempt());
    let mut actual = window_builder(arena.begin_attempt());
    let expected_input = rtl();
    let actual_input = rtl();
    assert!(!actual_input.rendered().row().reversed_p);
    assert!(matches!(
        actual_input.rendered().row().glyphs[GlyphArea::Text.index()][0].glyph_type,
        GlyphType::Char { ch: 'א' }
    ));
    install_borrowed_reference(&mut expected, &expected_input);
    install_measured_window_display_row(&mut actual, actual_input);
    let installed = actual.current_window_row(1).expect("RTL row installed");
    assert_eq!(Some(installed), expected.current_window_row(1));
    assert!(installed.reversed_p);
    assert!(matches!(
        installed.glyphs[GlyphArea::Text.index()][0].glyph_type,
        GlyphType::Char { ch: 'ב' }
    ));
    assert!(matches!(
        installed.glyphs[GlyphArea::Text.index()][1].glyph_type,
        GlyphType::Char { ch: 'א' }
    ));
}

#[test]
fn conflicting_prepared_face_refuses_before_replacing_a_window_row() {
    let arena = FrameFaceArena::default();
    let mut attempt = arena.begin_attempt();
    let mut first = Face::new(FaceId::new(101));
    first.font_family = "unpublished-first".into();
    let prepared = attempt
        .prepare_face(first)
        .expect("prepared before conflicting publication");
    let mut published = Face::new(FaceId::new(101));
    published.font_family = "published-other".into();
    let published = attempt
        .prepare_face(published)
        .expect("still unpublished ID");
    attempt
        .publish_face(&published)
        .expect("publish one rendering");
    let candidate = measured(
        WindowChromeKind::ModeLine,
        DisplayRowBoundsPolicy::MeasureIntrinsic,
        88.0,
        vec![prepared],
    );
    let mut builder = window_builder(attempt.clone());
    let before = builder.current_window_row(1).cloned();
    let face_before = builder.output_face(FaceId::new(101));
    let result = catch_unwind(AssertUnwindSafe(|| {
        install_measured_window_display_row(&mut builder, candidate)
    }));
    let payload = result.expect_err("conflicting prepared rendering must be refused");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        message.contains("published face must belong to the output attempt"),
        "{message}"
    );
    assert_eq!(builder.current_window_row(1), before.as_ref());
    assert_eq!(builder.output_face(FaceId::new(101)), face_before);
}
