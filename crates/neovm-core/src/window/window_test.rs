use super::*;
use crate::buffer::{CharLen, EmacsByteLen};
use neomacs_display_protocol::TransitionDirection;

#[test]
fn layout_variable_enum_covers_the_display_dirty_registry() {
    use crate::buffer::buffer::{DISPLAY_AFFECTING_BUFFER_SLOTS, DISPLAY_AFFECTING_GLOBAL_VARS};

    for name in DISPLAY_AFFECTING_BUFFER_SLOTS
        .iter()
        .chain(DISPLAY_AFFECTING_GLOBAL_VARS)
    {
        assert!(
            name.parse::<WindowLayoutVariable>().is_ok(),
            "display-affecting variable {name:?} must have a typed layout identity"
        );
    }
}

#[test]
fn window_end_state_preserves_one_atomic_record_across_invalidation() {
    let mut window = Window::new_leaf(WindowId(11), BufferId(1), Rect::new(0.0, 0.0, 800.0, 600.0));

    assert_eq!(window.window_end_state(), Some(WindowEndState::Unrecorded));

    window.set_window_end_from_positions(
        LispCharPos1::new(200),
        EmacsBytePos::new(240),
        LispCharPos1::new(150),
        EmacsBytePos::new(175),
        3,
    );

    let Some(WindowEndState::Current(record)) = window.window_end_state() else {
        panic!("completed redisplay should publish one current window-end record");
    };
    assert_eq!(record.char_offset_from_z(), CharLen::new(50));
    assert_eq!(record.byte_offset_from_z(), EmacsByteLen::new(65));
    assert_eq!(record.matrix_row(), MatrixRow0::new(3));

    window.invalidate_display_state();

    let Some(WindowEndState::Stale(record)) = window.window_end_state() else {
        panic!("invalidation should preserve the last presented record as stale");
    };
    assert_eq!(record.char_offset_from_z(), CharLen::new(50));
    assert_eq!(record.byte_offset_from_z(), EmacsByteLen::new(65));
    assert_eq!(record.matrix_row(), MatrixRow0::new(3));
    assert_eq!(
        window.window_end_charpos(LispCharPos1::new(200)),
        Some(LispCharPos1::new(150)),
        "UPDATE=nil semantics retain the last recorded offset"
    );
}

#[test]
fn prepared_presentation_publishes_one_atomic_window_redisplay_output() {
    use super::geometry::PresentationId;

    let mut frames = FrameManager::new();
    let frame_id = frames.create_frame("atomic-window-output", 800, 600, BufferId(1));
    let frame = frames.get_mut(frame_id).expect("frame");
    let window_id = frame.selected_window;
    let first_end = WindowEndRecord::from_positions(
        LispCharPos1::new(300),
        EmacsBytePos::new(360),
        LispCharPos1::new(120),
        EmacsBytePos::new(150),
        MatrixRow0::new(8),
    );
    let first_cursor = WindowCursorPos {
        x: 24,
        y: 32,
        row: 2,
        col: 3,
    };
    let first_snapshot = WindowDisplaySnapshot {
        window_id,
        window_end_record: Some(first_end),
        logical_cursor: Some(first_cursor),
        rows: vec![DisplayRowSnapshot {
            row: 0,
            start_buffer_pos: Some(LispCharPos1::new(10)),
            end_buffer_pos: Some(LispCharPos1::new(119)),
            ..DisplayRowSnapshot::default()
        }],
        ..WindowDisplaySnapshot::default()
    };

    frame
        .prepare_live_window_presentation(PresentationId::new(41), vec![first_snapshot])
        .expect("prepare first output");
    let first = frame
        .find_window(window_id)
        .expect("window")
        .redisplay_output()
        .expect("atomic output");
    assert_eq!(first.generation(), PresentationId::new(41));
    assert_eq!(first.window_end(), first_end);
    assert_eq!(
        first.visible_span(),
        Some(WindowVisibleBufferSpan::new(
            LispCharPos1::new(10),
            LispCharPos1::new(119)
        ))
    );
    assert_eq!(first.logical_cursor(), Some(first_cursor));

    let second_end = WindowEndRecord::from_positions(
        LispCharPos1::new(300),
        EmacsBytePos::new(360),
        LispCharPos1::new(180),
        EmacsBytePos::new(220),
        MatrixRow0::new(11),
    );
    let second_snapshot = WindowDisplaySnapshot {
        window_id,
        window_end_record: Some(second_end),
        rows: vec![DisplayRowSnapshot {
            row: 0,
            start_buffer_pos: Some(LispCharPos1::new(70)),
            end_buffer_pos: Some(LispCharPos1::new(179)),
            ..DisplayRowSnapshot::default()
        }],
        ..WindowDisplaySnapshot::default()
    };
    frame
        .prepare_live_window_presentation(PresentationId::new(42), vec![second_snapshot])
        .expect("prepare second output");
    let second = frame
        .find_window(window_id)
        .expect("window")
        .redisplay_output()
        .expect("replacement output");
    assert_eq!(second.generation(), PresentationId::new(42));
    assert_eq!(second.window_end(), second_end);
    assert_eq!(
        second.visible_span(),
        Some(WindowVisibleBufferSpan::new(
            LispCharPos1::new(70),
            LispCharPos1::new(179)
        ))
    );
    assert_eq!(second.logical_cursor(), None);
}

#[test]
fn snapshot_window_geometry_keeps_pixel_spaces_and_cell_origin_distinct() {
    use super::geometry::{Column, Line, PresentationGeometry, PresentationId};
    use neomacs_display_protocol::types::Rect as TransportRect;

    let mut window = Window::new_leaf(
        WindowId(11),
        BufferId(1),
        Rect::new(144.0, 24.0, 1831.0, 1172.0),
    );
    window.set_left_col(18);
    window.set_top_line(2);
    let snapshot = WindowDisplaySnapshot {
        window_id: WindowId(11),
        cell_origin: super::geometry::CellOrigin::new(18, 2),
        regions: PresentedWindowRegions {
            outer: TransportRect::new(144.0, 24.0, 1831.0, 1172.0),
            text_body: TransportRect::new(168.0, 41.0, 1807.0, 1138.0),
            ..PresentedWindowRegions::default()
        },
        regions_materialized: true,
        // Deliberately stale compatibility scalars: typed geometry must use
        // the explicit regions above instead.
        text_area_left_offset: 999,
        header_line_height: 88,
        tab_line_height: 99,
        points: vec![DisplayPointSnapshot {
            role: DisplayPointRole::Glyph,
            buffer_pos: LispCharPos1::ONE,
            x: 475,
            y: 340,
            width: 7,
            height: 17,
            row: 20,
            col: 66,
        }],
        body_rows: vec![PresentedBodyRowSnapshot {
            output_row: 20,
            body_row: 18,
            body_y: 323,
        }],
        ..WindowDisplaySnapshot::default()
    };

    // The published view must not reread mutable live geometry.
    window.set_bounds(Rect::new(999.0, 999.0, 1.0, 1.0));
    window.set_left_col(99);
    window.set_top_line(99);
    let presented = PresentationGeometry::new(FrameId(7), PresentationId::new(41), [snapshot])
        .expect("valid presented geometry");
    let geometry = presented
        .resolve(super::geometry::WindowGeometryQuery::new(
            PresentationId::new(41),
            window.id(),
        ))
        .expect("valid presented geometry");
    assert_eq!(geometry.presentation(), PresentationId::new(41));
    assert_eq!(geometry.cell_origin().column(), Column::new(18));
    assert_eq!(geometry.cell_origin().line(), Line::new(2));

    let body_origin_in_window = geometry.text_body_origin_in_window();
    assert_eq!(body_origin_in_window.window(), WindowId(11));
    assert_eq!(body_origin_in_window.x().get(), 24.0);
    assert_eq!(body_origin_in_window.y().get(), 17.0);

    let body_origin = geometry
        .text_body_origin_in_frame()
        .expect("body origin composes into frame space");
    assert_eq!(body_origin.x().get(), 168.0);
    assert_eq!(body_origin.y().get(), 41.0);

    let point = geometry
        .point_for_buffer_pos(LispCharPos1::ONE)
        .expect("valid snapshot geometry")
        .expect("visible point");
    let body_point = point.in_text_body();
    assert_eq!(body_point.x().get(), 475.0);
    assert_eq!(body_point.y().get(), 323.0);

    let frame_point = point.in_frame();
    assert_eq!(frame_point.frame(), FrameId(7));
    assert_eq!(frame_point.x().get(), 643.0);
    assert_eq!(frame_point.y().get(), 364.0);
}

#[test]
fn sealed_geometry_queries_reject_stale_presentations_and_use_explicit_regions() {
    use super::geometry::{
        GeometryQueryError, PresentationGeometry, PresentationId, WindowCoordinateQuery,
        WindowGeometryQuery, WindowRegion, WindowRegionBoundsQuery,
    };
    use neomacs_display_protocol::types::Rect as TransportRect;

    let frame_point = |x, y| {
        neomacs_display_protocol::PresentedFramePoint::from_px(x as f32, y as f32)
            .expect("valid frame point")
    };

    let window_id = WindowId(11);
    let presentation = PresentationId::new(41);
    let publication = PresentationGeometry::new(
        FrameId(7),
        presentation,
        [WindowDisplaySnapshot {
            window_id,
            regions: PresentedWindowRegions {
                outer: TransportRect::new(144.0, 24.0, 800.0, 600.0),
                text_body: TransportRect::new(180.0, 60.0, 700.0, 520.0),
                left_scroll_bar: Some(TransportRect::new(144.0, 60.0, 12.0, 520.0)),
                ..PresentedWindowRegions::default()
            },
            regions_materialized: true,
            text_area_left_offset: 999,
            header_line_height: 88,
            tab_line_height: 99,
            points: vec![DisplayPointSnapshot {
                role: DisplayPointRole::Glyph,
                buffer_pos: LispCharPos1::ONE,
                x: 10,
                y: 999,
                width: 7,
                height: 17,
                row: 8,
                col: 3,
            }],
            body_rows: vec![PresentedBodyRowSnapshot {
                output_row: 8,
                body_row: 2,
                body_y: 20,
            }],
            ..WindowDisplaySnapshot::default()
        }],
    )
    .expect("valid presented geometry");

    let error = publication
        .resolve(WindowGeometryQuery::new(PresentationId::new(40), window_id))
        .expect_err("stale presentation must not resolve");
    assert_eq!(
        error,
        GeometryQueryError::StalePresentation {
            requested: PresentationId::new(40),
            available: presentation,
        }
    );
    assert_eq!(
        publication
            .resolve(WindowCoordinateQuery::in_frame(
                presentation,
                WindowId(99),
                frame_point(190, 80),
            ))
            .expect_err("unknown window must not resolve"),
        GeometryQueryError::MissingWindow(WindowId(99))
    );

    let skipped = PresentationGeometry::new(
        FrameId(7),
        PresentationId::new(42),
        [WindowDisplaySnapshot {
            window_id,
            regions: PresentedWindowRegions {
                outer: TransportRect::new(144.0, 24.0, 800.0, 600.0),
                ..PresentedWindowRegions::default()
            },
            regions_materialized: false,
            ..WindowDisplaySnapshot::default()
        }],
    )
    .expect("valid skipped window publication");
    assert_eq!(
        skipped
            .resolve(WindowCoordinateQuery::in_frame(
                PresentationId::new(42),
                window_id,
                frame_point(190, 80),
            ))
            .expect_err("coordinate query requires materialized geometry"),
        GeometryQueryError::MissingMaterializedGeometry(window_id)
    );

    let body = publication
        .resolve(WindowRegionBoundsQuery::new(
            presentation,
            window_id,
            WindowRegion::TextBody,
        ))
        .expect("explicit body region");
    assert_eq!(body.origin().x().get(), 180.0);
    assert_eq!(body.origin().y().get(), 60.0);
    assert_eq!(body.width().get(), 700.0);
    assert_eq!(body.height().get(), 520.0);

    for query in [
        WindowCoordinateQuery::in_text_body(presentation, window_id, 10, 56),
        WindowCoordinateQuery::in_whole_window(presentation, window_id, 46, 56),
        WindowCoordinateQuery::in_frame(presentation, window_id, frame_point(190, 80)),
    ] {
        let point = publication.resolve(query).expect("visible coordinate");
        assert_eq!(point.buffer_pos(), LispCharPos1::ONE);
        assert_eq!(point.in_text_body().x().get(), 10.0);
        assert_eq!(point.in_text_body().y().get(), 20.0);
    }

    let error = publication
        .resolve(WindowCoordinateQuery::in_frame(
            PresentationId::new(40),
            window_id,
            frame_point(190, 80),
        ))
        .expect_err("stale coordinate query must not resolve");
    assert_eq!(
        error,
        GeometryQueryError::StalePresentation {
            requested: PresentationId::new(40),
            available: presentation,
        }
    );
}

#[test]
fn prepare_activate_replaces_geometry_and_presentation_together() {
    use super::geometry::{PresentationId, PresentationPrepareError};

    let mut manager = FrameManager::new();
    let frame_id = manager.create_frame("F1", 800, 600, BufferId(1));
    let frame = manager.get_mut(frame_id).expect("frame");
    let window_id = frame.selected_window;

    frame
        .prepare_and_activate_display_presentation_for_test(
            PresentationId::new(41),
            vec![WindowDisplaySnapshot {
                window_id,
                text_area_left_offset: 8,
                ..WindowDisplaySnapshot::default()
            }],
        )
        .expect("first presentation");
    assert_eq!(frame.active_presentation(), Some(PresentationId::new(41)));
    assert_eq!(
        frame
            .redisplay_snapshot(window_id)
            .expect("published snapshot")
            .text_area_left_offset,
        8
    );

    frame.remove_redisplay_snapshot(window_id);
    assert_eq!(frame.active_presentation(), Some(PresentationId::new(41)));
    assert!(frame.redisplay_snapshot(window_id).is_none());

    assert_eq!(
        frame.prepare_and_activate_display_presentation_for_test(
            PresentationId::new(41),
            Vec::new()
        ),
        Err(PresentationPrepareError::ReusedPresentation(
            PresentationId::new(41)
        ))
    );

    frame
        .prepare_and_activate_display_presentation_for_test(PresentationId::new(42), Vec::new())
        .expect("newer presentation");
    assert_eq!(frame.active_presentation(), Some(PresentationId::new(42)));
    assert!(frame.redisplay_snapshot(window_id).is_none());

    frame.replace_redisplay_cache_for_test(Vec::new());
    assert_eq!(frame.active_presentation(), Some(PresentationId::new(42)));
    assert!(frame.retire_display_presentation(PresentationId::new(42)));
    assert_eq!(frame.active_presentation(), None);
}

#[test]
fn prepared_display_presentation_does_not_replace_active_geometry() {
    use super::geometry::PresentationId;

    let mut manager = FrameManager::new();
    let frame_id = manager.create_frame("prepared-presentation", 800, 600, BufferId(1));
    let frame = manager.get_mut(frame_id).expect("frame");
    let window_id = frame.selected_window;

    frame
        .prepare_live_window_presentation(
            PresentationId::new(41),
            vec![WindowDisplaySnapshot {
                window_id,
                text_area_left_offset: 8,
                ..WindowDisplaySnapshot::default()
            }],
        )
        .expect("prepare first presentation");
    assert_eq!(frame.active_presentation(), None);
    assert_eq!(
        frame
            .redisplay_snapshot(window_id)
            .expect("prepared redisplay cache")
            .text_area_left_offset,
        8
    );

    assert_eq!(
        frame
            .activate_display_presentation(PresentationId::new(41))
            .expect("activate first presentation"),
        None
    );
    assert_eq!(frame.active_presentation(), Some(PresentationId::new(41)));
    assert_eq!(
        frame
            .redisplay_snapshot(window_id)
            .expect("active snapshot")
            .text_area_left_offset,
        8
    );

    frame
        .prepare_live_window_presentation(
            PresentationId::new(42),
            vec![WindowDisplaySnapshot {
                window_id,
                text_area_left_offset: 24,
                ..WindowDisplaySnapshot::default()
            }],
        )
        .expect("prepare replacement");
    assert_eq!(frame.active_presentation(), Some(PresentationId::new(41)));
    assert_eq!(
        frame
            .redisplay_snapshot(window_id)
            .expect("latest redisplay cache")
            .text_area_left_offset,
        24
    );
    assert_eq!(
        frame
            .active_presentation_geometry()
            .expect("old presentation remains active")
            .presentation(),
        PresentationId::new(41)
    );

    assert!(frame.discard_display_presentation(PresentationId::new(42)));
    assert_eq!(frame.active_presentation(), Some(PresentationId::new(41)));
}

#[test]
fn geometry_only_snapshot_is_interactive_but_not_live_redisplay_evidence() {
    use super::geometry::{PresentationId, PresentationPrepareError};

    let mut manager = FrameManager::new();
    let frame_id = manager.create_frame("temporary-echo-presentation", 800, 600, BufferId(1));
    let frame = manager.get_mut(frame_id).expect("frame");
    let window_id = frame.selected_window;

    frame
        .prepare_live_window_presentation(
            PresentationId::new(41),
            vec![WindowDisplaySnapshot {
                window_id,
                text_area_left_offset: 8,
                ..WindowDisplaySnapshot::default()
            }],
        )
        .expect("prepare live presentation");
    frame
        .activate_display_presentation(PresentationId::new(41))
        .expect("activate live presentation");

    frame
        .prepare_display_presentation(
            PresentationId::new(42),
            vec![WindowPresentationSnapshot::GeometryOnly(
                WindowDisplaySnapshot {
                    window_id,
                    text_area_left_offset: 24,
                    ..WindowDisplaySnapshot::default()
                },
            )],
        )
        .expect("prepare temporary echo geometry");
    frame
        .activate_display_presentation(PresentationId::new(42))
        .expect("activate temporary echo geometry");

    assert_eq!(
        frame
            .active_window_presentation(window_id)
            .expect("interaction geometry keeps the temporary window")
            .display_snapshot()
            .text_area_left_offset,
        24
    );
    assert_eq!(
        frame
            .redisplay_snapshot(window_id)
            .expect("temporary geometry preserves prior live evidence")
            .text_area_left_offset,
        8
    );
    assert_eq!(
        frame.prepare_display_presentation(
            PresentationId::new(42),
            vec![WindowPresentationSnapshot::LiveWindow(
                WindowDisplaySnapshot {
                    window_id,
                    text_area_left_offset: 24,
                    ..WindowDisplaySnapshot::default()
                },
            )],
        ),
        Err(PresentationPrepareError::ReusedPresentation(
            PresentationId::new(42)
        )),
        "an active identity cannot be reused with a different publication domain"
    );
}

#[test]
fn preparing_accepted_presentation_commits_live_window_output() {
    use super::geometry::PresentationId;

    let mut manager = FrameManager::new();
    let frame_id = manager.create_frame("accepted-output", 800, 600, BufferId(1));
    let frame = manager.get_mut(frame_id).expect("frame");
    let window_id = frame.selected_window;
    let logical_cursor = WindowCursorPos {
        x: 44,
        y: 29,
        row: 1,
        col: 8,
    };

    frame
        .prepare_live_window_presentation(
            PresentationId::new(41),
            vec![WindowDisplaySnapshot {
                window_id,
                logical_cursor: Some(logical_cursor),
                rows: vec![DisplayRowSnapshot {
                    row: 1,
                    y: 29,
                    height: 16,
                    start_x: 0,
                    start_col: 0,
                    end_x: 44,
                    end_col: 8,
                    start_buffer_pos: Some(crate::buffer::LispCharPos1::ONE),
                    end_buffer_pos: Some(crate::buffer::LispCharPos1::new(8)),
                    fringe: Default::default(),
                }],
                ..WindowDisplaySnapshot::default()
            }],
        )
        .expect("accepted presentation");

    let display = frame
        .find_window(window_id)
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(display.cursor, Some(logical_cursor));
    assert_eq!(display.output_cursor, Some(logical_cursor));
}

#[test]
fn discarded_display_presentation_cannot_be_activated() {
    use super::geometry::{PresentationActivateError, PresentationId, PresentationPrepareError};

    let mut manager = FrameManager::new();
    let frame_id = manager.create_frame("discarded-presentation", 800, 600, BufferId(1));
    let frame = manager.get_mut(frame_id).expect("frame");

    frame
        .prepare_live_window_presentation(PresentationId::new(41), Vec::new())
        .expect("prepare presentation");
    assert!(frame.discard_display_presentation(PresentationId::new(41)));
    assert_eq!(
        frame.prepare_live_window_presentation(PresentationId::new(41), Vec::new()),
        Err(PresentationPrepareError::ReusedPresentation(
            PresentationId::new(41)
        ))
    );
    assert_eq!(
        frame.activate_display_presentation(PresentationId::new(41)),
        Err(PresentationActivateError::UnknownPresentation(
            PresentationId::new(41)
        ))
    );
}

#[test]
fn presentation_identity_rejects_zero() {
    use super::geometry::PresentationId;

    assert_eq!(PresentationId::try_new(0), None);
    assert_eq!(PresentationId::try_new(7).map(PresentationId::get), Some(7));
}

#[test]
fn presented_child_placement_uses_immediate_parent_coordinates_without_chrome_or_desktop_offsets() {
    use super::geometry::PresentationId;

    let mut manager = FrameManager::new();
    let root = manager.create_frame("root-placement", 800, 600, BufferId(1));
    let child = manager.create_frame("child-placement", 300, 200, BufferId(1));
    let nested = manager.create_frame("nested-placement", 100, 80, BufferId(1));
    {
        let root_frame = manager.get_mut(root).unwrap();
        root_frame.left_pos = 900;
        root_frame.top_pos = 700;
        root_frame.menu_bar_height = 30;
        root_frame.tool_bar_height = 40;
        root_frame
            .prepare_and_activate_display_presentation_for_test(PresentationId::new(40), vec![])
            .unwrap();
    }
    {
        let child_frame = manager.get_mut(child).unwrap();
        child_frame.parent_frame = Value::make_frame(root.0);
        child_frame.left_pos = 100;
        child_frame.top_pos = 80;
        child_frame
            .prepare_and_activate_display_presentation_for_test(PresentationId::new(41), vec![])
            .unwrap();
    }
    {
        let nested_frame = manager.get_mut(nested).unwrap();
        nested_frame.parent_frame = Value::make_frame(child.0);
        nested_frame.left_pos = 15;
        nested_frame.top_pos = 12;
        nested_frame
            .prepare_and_activate_display_presentation_for_test(PresentationId::new(42), vec![])
            .unwrap();
    }

    let child_place = manager
        .place_active_frame(child, PresentationId::new(41))
        .unwrap();
    assert_eq!(
        (
            child_place.parent_relative().x(),
            child_place.parent_relative().y()
        ),
        (100.0, 80.0)
    );
    assert_eq!(
        (
            child_place.root_relative().x(),
            child_place.root_relative().y()
        ),
        (100.0, 80.0)
    );
    let nested_place = manager
        .place_active_frame(nested, PresentationId::new(42))
        .unwrap();
    assert_eq!(
        (
            nested_place.parent_relative().x(),
            nested_place.parent_relative().y()
        ),
        (15.0, 12.0)
    );
    assert_eq!(
        (
            nested_place.root_relative().x(),
            nested_place.root_relative().y()
        ),
        (115.0, 92.0)
    );
    assert!(matches!(
        manager.place_active_frame(nested, PresentationId::new(41)),
        Err(neomacs_display_protocol::PlaceChildError::StalePresentation { .. })
    ));
}

#[test]
fn popup_anchor_translates_with_side_window_without_changing_body_local_cursor_geometry() {
    use super::geometry::{PresentationId, WindowGeometryQuery};
    use neomacs_display_protocol::types::Rect as TransportRect;

    fn scenario(body_left: f32) -> ((f32, f32, f32, f32), (f32, f32), (f32, f32)) {
        let mut manager = FrameManager::new();
        let root = manager.create_frame("popup-parent", 800, 600, BufferId(1));
        let popup = manager.create_frame("corfu-popup", 240, 160, BufferId(1));
        let window = manager.get(root).unwrap().selected_window;
        let presentation = PresentationId::new(40);
        manager
            .get_mut(root)
            .unwrap()
            .prepare_and_activate_display_presentation_for_test(
                presentation,
                vec![WindowDisplaySnapshot {
                    window_id: window,
                    regions: PresentedWindowRegions {
                        outer: TransportRect::new(body_left, 50.0, 536.0, 500.0),
                        text_body: TransportRect::new(body_left, 50.0, 536.0, 500.0),
                        ..PresentedWindowRegions::default()
                    },
                    regions_materialized: true,
                    points: vec![DisplayPointSnapshot {
                        role: DisplayPointRole::Glyph,
                        buffer_pos: LispCharPos1::ONE,
                        x: 80,
                        y: 96,
                        width: 8,
                        height: 16,
                        row: 6,
                        col: 10,
                    }],
                    body_rows: vec![PresentedBodyRowSnapshot {
                        output_row: 6,
                        body_row: 6,
                        body_y: 96,
                    }],
                    ..WindowDisplaySnapshot::default()
                }],
            )
            .unwrap();

        let geometry = manager
            .get(root)
            .unwrap()
            .active_presentation_geometry()
            .unwrap()
            .resolve(WindowGeometryQuery::new(presentation, window))
            .unwrap();
        let body_origin = geometry.text_body_origin_in_frame().unwrap();
        let cursor = geometry
            .point_for_buffer_pos(LispCharPos1::ONE)
            .unwrap()
            .unwrap();
        let cursor_local = cursor.in_text_body();
        let anchor = (
            body_origin.x().get() + cursor_local.x().get(),
            body_origin.y().get() + cursor_local.y().get() + cursor.height().get(),
        );

        let popup_frame = manager.get_mut(popup).unwrap();
        popup_frame.parent_frame = Value::make_frame(root.0);
        popup_frame.left_pos = anchor.0 as i64;
        popup_frame.top_pos = anchor.1 as i64;
        popup_frame
            .prepare_and_activate_display_presentation_for_test(PresentationId::new(41), vec![])
            .unwrap();
        let placed = manager
            .place_active_frame(popup, PresentationId::new(41))
            .unwrap();

        (
            (
                cursor_local.x().get(),
                cursor_local.y().get(),
                cursor.width().get(),
                cursor.height().get(),
            ),
            (body_origin.x().get(), body_origin.y().get()),
            (placed.root_relative().x(), placed.root_relative().y()),
        )
    }

    let with_side = scenario(264.0);
    assert_eq!(with_side.0, (80.0, 96.0, 8.0, 16.0));
    assert_eq!(with_side.1, (264.0, 50.0));
    assert_eq!(with_side.2, (344.0, 162.0));

    let without_side = scenario(24.0);
    assert_eq!(without_side.0, with_side.0);
    assert_eq!(without_side.1, (24.0, 50.0));
    assert_eq!(without_side.2, (104.0, 162.0));
    assert_eq!(without_side.1.0 - with_side.1.0, -240.0);
    assert_eq!(without_side.2.0 - with_side.2.0, -240.0);
}

#[test]
fn active_visual_anchors_are_semantic_and_presentation_qualified() {
    use super::geometry::{
        AnchorEdge, GeometryQueryError, PresentationId, VisualAnchor, VisualAnchorQuery,
        WindowRegion,
    };
    use neomacs_display_protocol::types::Rect as TransportRect;

    let mut manager = FrameManager::new();
    let frame_id = manager.create_frame("anchor-parent", 800, 600, BufferId(1));
    let window = manager.get(frame_id).unwrap().selected_window;
    let presentation = PresentationId::new(51);
    manager
        .get_mut(frame_id)
        .unwrap()
        .prepare_live_window_presentation(
            presentation,
            vec![WindowDisplaySnapshot {
                window_id: window,
                regions: PresentedWindowRegions {
                    outer: TransportRect::new(240.0, 40.0, 560.0, 520.0),
                    text_body: TransportRect::new(264.0, 50.0, 536.0, 500.0),
                    ..PresentedWindowRegions::default()
                },
                regions_materialized: true,
                logical_cursor: Some(WindowCursorPos {
                    x: 80,
                    y: 96,
                    row: 6,
                    col: 10,
                }),
                points: vec![DisplayPointSnapshot {
                    role: DisplayPointRole::Glyph,
                    buffer_pos: LispCharPos1::ONE,
                    x: 80,
                    y: 96,
                    width: 8,
                    height: 16,
                    row: 6,
                    col: 10,
                }],
                body_rows: vec![PresentedBodyRowSnapshot {
                    output_row: 6,
                    body_row: 6,
                    body_y: 96,
                }],
                ..WindowDisplaySnapshot::default()
            }],
        )
        .unwrap();

    let cursor_query = VisualAnchorQuery::new(presentation, VisualAnchor::CursorBottom { window });
    assert_eq!(
        manager
            .get(frame_id)
            .unwrap()
            .resolve_active_visual_anchor(cursor_query),
        Err(GeometryQueryError::NotYetActive { frame: frame_id })
    );

    manager
        .get_mut(frame_id)
        .unwrap()
        .activate_display_presentation(presentation)
        .unwrap();
    let cursor = manager
        .get(frame_id)
        .unwrap()
        .resolve_active_visual_anchor(cursor_query)
        .unwrap();
    assert_eq!(cursor.edge(), AnchorEdge::Bottom);
    assert_eq!(
        (cursor.x().get(), cursor.y().get()),
        (344.0, 162.0),
        "cursor bottom includes side-window translation and text-body origin"
    );

    let position = manager
        .get(frame_id)
        .unwrap()
        .resolve_active_visual_anchor(VisualAnchorQuery::new(
            presentation,
            VisualAnchor::BufferPositionBottom {
                window,
                position: LispCharPos1::ONE,
            },
        ))
        .unwrap();
    assert_eq!(position, cursor);

    let right_edge = manager
        .get(frame_id)
        .unwrap()
        .resolve_active_visual_anchor(VisualAnchorQuery::new(
            presentation,
            VisualAnchor::WindowRegionEdge {
                window,
                region: WindowRegion::TextBody,
                edge: AnchorEdge::Right,
            },
        ))
        .unwrap();
    assert_eq!((right_edge.x().get(), right_edge.y().get()), (800.0, 50.0));

    assert_eq!(
        manager
            .get(frame_id)
            .unwrap()
            .resolve_active_visual_anchor(VisualAnchorQuery::new(
                PresentationId::new(50),
                VisualAnchor::CursorBottom { window },
            )),
        Err(GeometryQueryError::StalePresentation {
            requested: PresentationId::new(50),
            available: PresentationId::new(51),
        })
    );
}

#[test]
fn duplicate_windows_reject_candidate_without_replacing_publication() {
    use super::geometry::{GeometryError, PresentationId, PresentationPrepareError};

    let mut manager = FrameManager::new();
    let frame_id = manager.create_frame("duplicate-publication", 800, 600, BufferId(1));
    let frame = manager.get_mut(frame_id).expect("frame");
    let window_id = frame.selected_window;
    frame
        .prepare_and_activate_display_presentation_for_test(
            PresentationId::new(1),
            vec![WindowDisplaySnapshot {
                window_id,
                ..WindowDisplaySnapshot::default()
            }],
        )
        .expect("initial publication");

    let result = frame.prepare_and_activate_display_presentation_for_test(
        PresentationId::new(2),
        vec![
            WindowDisplaySnapshot {
                window_id,
                ..WindowDisplaySnapshot::default()
            },
            WindowDisplaySnapshot {
                window_id,
                ..WindowDisplaySnapshot::default()
            },
        ],
    );

    assert_eq!(
        result,
        Err(PresentationPrepareError::InvalidGeometry(
            GeometryError::DuplicateWindow(window_id)
        ))
    );
    assert_eq!(frame.active_presentation(), Some(PresentationId::new(1)));
}

#[test]
fn presented_positions_require_body_local_row_facts() {
    use super::geometry::{GeometryError, PresentationGeometry, PresentationId};

    let window_id = WindowId(11);
    let result = PresentationGeometry::new(
        FrameId(7),
        PresentationId::new(1),
        [WindowDisplaySnapshot {
            window_id,
            points: vec![DisplayPointSnapshot {
                role: DisplayPointRole::Glyph,
                buffer_pos: LispCharPos1::ONE,
                x: 0,
                y: 10,
                width: 8,
                height: 16,
                row: 2,
                col: 0,
            }],
            ..WindowDisplaySnapshot::default()
        }],
    );

    assert_eq!(
        result,
        Err(GeometryError::MissingBodyRow {
            window: window_id,
            output_row: 2,
        })
    );
}

#[test]
fn legacy_unowned_snapshots_do_not_create_an_authoritative_geometry_view() {
    let mut manager = FrameManager::new();
    let frame_id = manager.create_frame("legacy", 800, 600, BufferId(1));
    let frame = manager.get_mut(frame_id).expect("frame");
    let window_id = frame.selected_window;
    frame.replace_redisplay_cache_for_test(vec![WindowDisplaySnapshot {
        window_id,
        ..WindowDisplaySnapshot::default()
    }]);

    assert!(frame.redisplay_snapshot(window_id).is_some());
    assert!(frame.active_presentation_geometry().is_none());
}
use crate::buffer::LispCharPos1;

#[test]
fn window_cursor_kind_codes_match_gnu_text_cursor_kinds() {
    crate::test_utils::init_test_tracing();

    let cases = [
        (WindowCursorKind::NoCursor, -1),
        (WindowCursorKind::FilledBox, 0),
        (WindowCursorKind::HollowBox, 1),
        (WindowCursorKind::Bar, 2),
        (WindowCursorKind::Hbar, 3),
    ];

    for (kind, code) in cases {
        assert_eq!(kind.gnu_code(), code);
        assert_eq!(WindowCursorKind::from_gnu_code(code), Some(kind));
    }

    assert_eq!(WindowCursorKind::from_gnu_code(-2), None);
    assert_eq!(WindowCursorKind::from_gnu_code(4), None);
}

#[test]
fn create_frame_and_window() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let frame = mgr.get(fid).unwrap();

    assert_eq!(frame.window_count(), 1);
    assert!(frame.selected_window().is_some());
    assert!(frame.selected_window().unwrap().is_leaf());
    assert_eq!(frame.parameter("tab-bar-lines"), Some(Value::fixnum(0)));
}

#[test]
fn gui_default_parameters_seed_scroll_bar_and_fringe_chrome() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let frame = mgr.get_mut(fid).expect("frame");
    frame.install_gnu_gui_default_parameters();

    // GNU's gui_default_parameter (xfns.c) seeds these on every GUI frame.
    // Without them, `(frame-parameter f 'vertical-scroll-bars)` reports nil even
    // though the layout already reserves and draws the scroll bar — the frame
    // parameter lied about what is on screen. Each seeded value equals the layout
    // fallback in window/display.rs, so this is a reporting fix, not a geometry
    // change. scroll-bar-width stays unseeded on purpose: its fallback tracks the
    // live char width, so a fixed seed would go stale across a font change.
    assert_eq!(
        frame.known_parameter(FrameParam::VerticalScrollBars),
        Some(Value::symbol("right"))
    );
    assert_eq!(
        frame.known_parameter(FrameParam::HorizontalScrollBars),
        Some(Value::NIL)
    );
    assert_eq!(
        frame.known_parameter(FrameParam::LeftFringe),
        Some(Value::fixnum(8))
    );
    assert_eq!(
        frame.known_parameter(FrameParam::RightFringe),
        Some(Value::fixnum(8))
    );
    assert_eq!(
        frame.known_parameter(FrameParam::InternalBorderWidth),
        Some(Value::fixnum(0))
    );
    assert_eq!(
        frame.known_parameter(FrameParam::BorderWidth),
        Some(Value::fixnum(0))
    );
}

#[test]
fn selected_window_accessor_includes_minibuffer_leaf() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let minibuffer = mgr
        .get(fid)
        .and_then(|frame| frame.minibuffer_window)
        .expect("frame minibuffer");

    {
        let frame = mgr.get_mut(fid).expect("frame");
        assert!(frame.select_window(minibuffer));
    }

    let frame = mgr.get(fid).expect("frame");
    assert_eq!(
        frame.selected_window().map(|window| window.id()),
        Some(minibuffer)
    );
}

#[test]
fn deleting_child_frame_that_shares_minibuffer_does_not_delete_owner_minibuffer() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let root_id = mgr.create_frame("F1", 80, 25, BufferId(1));
    let child_id = mgr.create_frame("child", 10, 4, BufferId(1));
    let root_minibuffer = mgr
        .get(root_id)
        .and_then(|frame| frame.minibuffer_window)
        .expect("root minibuffer");

    {
        let child = mgr.get_mut(child_id).expect("child frame");
        child.parent_frame = Value::make_frame(root_id.0);
        child.minibuffer_window = Some(root_minibuffer);
        child.minibuffer_leaf = None;
    }

    assert_eq!(mgr.find_window_frame_id(root_minibuffer), Some(root_id));
    assert_eq!(
        mgr.find_valid_window_frame_id(root_minibuffer),
        Some(root_id)
    );
    assert!(mgr.delete_frame(child_id).was_deleted());
    assert!(!mgr.deleted_windows.contains(&root_minibuffer));
    assert_eq!(mgr.find_window_frame_id(root_minibuffer), Some(root_id));
    assert!(mgr.lookup_window(root_minibuffer).is_some());
}

#[test]
fn gui_internal_border_width_insets_root_and_minibuffer_geometry() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 100, 80, BufferId(1));

    {
        let frame = mgr.get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("x")));
        frame.set_parameter(Value::symbol("internal-border-width"), Value::fixnum(4));
        frame.sync_window_area_bounds();
    }

    let frame = mgr.get(fid).expect("frame");
    assert_eq!(*frame.root_window.bounds(), Rect::new(4.0, 4.0, 92.0, 56.0));
    assert_eq!(
        *frame.minibuffer_leaf.as_ref().expect("minibuffer").bounds(),
        Rect::new(4.0, 60.0, 92.0, 16.0)
    );
}

#[test]
fn tty_internal_border_width_parameters_do_not_inset_geometry() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 100, 80, BufferId(1));

    {
        let frame = mgr.get_mut(fid).expect("frame");
        frame.set_parameter(Value::symbol("internal-border-width"), Value::fixnum(4));
        frame.set_parameter(Value::symbol("child-frame-border-width"), Value::fixnum(2));
        frame.sync_window_area_bounds();
    }

    let frame = mgr.get(fid).expect("frame");
    assert_eq!(frame.internal_border_width(), 0);
    assert_eq!(frame.frame_child_frame_border_width(), 0);
    assert_eq!(
        *frame.root_window.bounds(),
        Rect::new(0.0, 0.0, 100.0, 64.0)
    );
    assert_eq!(
        *frame.minibuffer_leaf.as_ref().expect("minibuffer").bounds(),
        Rect::new(0.0, 64.0, 100.0, 16.0)
    );
}

#[test]
fn nil_window_system_parameter_does_not_make_tty_frame_graphic() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 100, 80, BufferId(1));

    {
        let frame = mgr.get_mut(fid).expect("frame");
        frame.set_parameter(Value::symbol("window-system"), Value::NIL);
        frame.set_parameter(Value::symbol("internal-border-width"), Value::fixnum(4));
        frame.sync_window_area_bounds();
    }

    let frame = mgr.get(fid).expect("frame");
    assert_eq!(frame.effective_window_system(), None);
    assert_eq!(frame.internal_border_width(), 0);
    assert_eq!(
        *frame.root_window.bounds(),
        Rect::new(0.0, 0.0, 100.0, 64.0)
    );
}

#[test]
fn nil_internal_window_system_does_not_make_tty_frame_graphic() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 100, 80, BufferId(1));

    {
        let frame = mgr.get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::NIL));
        frame.set_parameter(Value::symbol("internal-border-width"), Value::fixnum(4));
        frame.sync_window_area_bounds();
    }

    let frame = mgr.get(fid).expect("frame");
    assert_eq!(frame.effective_window_system(), None);
    assert_eq!(frame.internal_border_width(), 0);
    assert_eq!(
        *frame.root_window.bounds(),
        Rect::new(0.0, 0.0, 100.0, 64.0)
    );
}

#[test]
fn gui_child_frame_border_width_acts_as_child_internal_border() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let parent_id = mgr.create_frame("parent", 100, 80, BufferId(1));
    let child_id = mgr.create_frame("child", 40, 30, BufferId(1));
    let parent_minibuffer = mgr
        .get(parent_id)
        .and_then(|frame| frame.minibuffer_window)
        .expect("parent minibuffer");

    {
        let parent = mgr.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("x")));
    }

    {
        let child = mgr.get_mut(child_id).expect("child frame");
        child.set_window_system(Some(Value::symbol("x")));
        child.parent_frame = Value::make_frame(parent_id.0);
        child.minibuffer_window = Some(parent_minibuffer);
        child.minibuffer_leaf = None;
        child.set_parameter(Value::symbol("internal-border-width"), Value::fixnum(4));
        child.set_parameter(Value::symbol("child-frame-border-width"), Value::fixnum(2));
        child.sync_window_area_bounds();
    }

    let child = mgr.get(child_id).expect("child frame");
    assert_eq!(child.internal_border_width(), 2);
    assert_eq!(*child.root_window.bounds(), Rect::new(2.0, 2.0, 36.0, 26.0));
}

#[test]
fn tty_child_frame_border_width_parameters_do_not_inset_geometry() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let parent_id = mgr.create_frame("parent", 100, 80, BufferId(1));
    let child_id = mgr.create_frame("child", 40, 30, BufferId(1));
    let parent_minibuffer = mgr
        .get(parent_id)
        .and_then(|frame| frame.minibuffer_window)
        .expect("parent minibuffer");

    {
        let child = mgr.get_mut(child_id).expect("child frame");
        child.parent_frame = Value::make_frame(parent_id.0);
        child.minibuffer_window = Some(parent_minibuffer);
        child.minibuffer_leaf = None;
        child.set_parameter(Value::symbol("internal-border-width"), Value::fixnum(4));
        child.set_parameter(Value::symbol("child-frame-border-width"), Value::fixnum(2));
        child.sync_window_area_bounds();
    }

    let child = mgr.get(child_id).expect("child frame");
    assert_eq!(child.internal_border_width(), 0);
    assert_eq!(child.frame_child_frame_border_width(), 0);
    assert_eq!(*child.root_window.bounds(), Rect::new(0.0, 0.0, 40.0, 30.0));
}

#[test]
fn render_frame_tree_returns_root_relative_bottom_to_top_nodes() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let root_id = mgr.create_frame("root", 80, 25, BufferId(1));
    let back_id = mgr.create_frame("back", 10, 4, BufferId(1));
    let front_id = mgr.create_frame("front", 10, 4, BufferId(1));
    let nested_id = mgr.create_frame("nested", 4, 2, BufferId(1));

    {
        let back = mgr.get_mut(back_id).expect("back child");
        back.parent_frame = Value::make_frame(root_id.0);
        back.left_pos = 10;
        back.top_pos = 20;
        back.z_order = 1;
    }
    {
        let front = mgr.get_mut(front_id).expect("front child");
        front.parent_frame = Value::make_frame(root_id.0);
        front.left_pos = 30;
        front.top_pos = 40;
        front.z_order = 2;
    }
    {
        let nested = mgr.get_mut(nested_id).expect("nested child");
        nested.parent_frame = Value::make_frame(front_id.0);
        nested.left_pos = 3;
        nested.top_pos = 4;
        nested.z_order = 1;
    }

    let tree = mgr
        .render_frame_tree(nested_id, RenderFrameVisibility::VisibleOnly)
        .expect("render frame tree");
    let ids: Vec<_> = tree
        .frames_bottom_to_top
        .iter()
        .map(|node| node.frame_id)
        .collect();

    assert_eq!(tree.root_id, root_id);
    assert_eq!(ids, vec![root_id, back_id, front_id, nested_id]);
    let nested = tree
        .frames_bottom_to_top
        .iter()
        .find(|node| node.frame_id == nested_id)
        .expect("nested node");
    assert_eq!(nested.parent_id, Some(front_id));
    assert_eq!(nested.origin_in_root_x, 33.0);
    assert_eq!(nested.origin_in_root_y, 44.0);
}

#[test]
fn render_frame_forest_returns_each_visible_native_window_tree_once() {
    let mut mgr = FrameManager::new();
    let first = mgr.create_frame("first", 80, 25, BufferId(1));
    let child = mgr.create_frame("child", 20, 10, BufferId(1));
    let second = mgr.create_frame("second", 80, 25, BufferId(1));
    let hidden = mgr.create_frame("hidden", 80, 25, BufferId(1));
    let tty = mgr.create_frame("tty", 80, 25, BufferId(1));

    for frame_id in [first, second, hidden] {
        mgr.get_mut(frame_id)
            .expect("native frame")
            .set_window_system(Some(Value::symbol("neo")));
    }
    mgr.get_mut(child).expect("child").parent_frame = Value::make_frame(first.0);
    mgr.get_mut(hidden).expect("hidden frame").visibility = FrameVisibility::Invisible;

    let forest = mgr.render_frame_forest(
        RenderFrameScope::AllNativeWindowTrees,
        RenderFrameVisibility::VisibleOnly,
    );

    assert_eq!(
        forest.iter().map(|tree| tree.root_id).collect::<Vec<_>>(),
        vec![first, second]
    );
    assert_eq!(
        forest[0]
            .frames_bottom_to_top
            .iter()
            .map(|node| node.frame_id)
            .collect::<Vec<_>>(),
        vec![first, child]
    );
    assert!(
        forest
            .iter()
            .flat_map(|tree| &tree.frames_bottom_to_top)
            .all(|node| node.frame_id != hidden && node.frame_id != tty)
    );
}

#[test]
fn child_frame_origin_is_relative_to_parent_child_frame_viewport() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let root_id = mgr.create_frame("root", 800, 600, BufferId(1));
    let child_id = mgr.create_frame("child", 160, 90, BufferId(1));
    let nested_id = mgr.create_frame("nested", 80, 40, BufferId(1));

    {
        let root = mgr.get_mut(root_id).expect("root");
        root.menu_bar_height = 33;
        root.tool_bar_height = 44;
        root.compact_bar_height = 12;
        root.tab_bar_height = 22;
    }
    {
        let child = mgr.get_mut(child_id).expect("child");
        child.parent_frame = Value::make_frame(root_id.0);
        child.left_pos = 20;
        child.top_pos = 20;
        child.menu_bar_height = 11;
    }
    {
        let nested = mgr.get_mut(nested_id).expect("nested");
        nested.parent_frame = Value::make_frame(child_id.0);
        nested.left_pos = 5;
        nested.top_pos = 7;
    }

    assert_eq!(mgr.frame_origin_in_root(child_id), Some((20.0, 131.0)));
    assert_eq!(mgr.frame_origin_in_root(nested_id), Some((25.0, 149.0)));
}

#[test]
fn frame_manager_gc_traces_name_icon_name_and_title_values() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let focus_fid = mgr.create_frame("F2", 800, 600, BufferId(2));
    {
        let frame = mgr.get_mut(fid).expect("frame");
        frame.icon_name = Value::string("Frame Icon");
        frame.title = Value::string("Frame Title");
        frame.focus_frame = Value::make_frame(focus_fid.0);
    }

    let frame = mgr.get(fid).expect("frame");
    let name = frame.name_value();
    let icon_name = frame.icon_name_value();
    let title = frame.title_value();
    let focus_frame = frame.focus_frame_value();

    let mut roots = Vec::new();
    mgr.trace_roots(&mut roots);

    assert!(roots.contains(&name));
    assert!(roots.contains(&icon_name));
    assert!(roots.contains(&title));
    assert!(roots.contains(&focus_frame));
}

#[test]
fn frame_manager_gc_traces_prepared_and_active_chrome_strings() {
    let mut mgr = FrameManager::new();
    let frame_id = mgr.create_frame("chrome-roots", 800, 600, BufferId(1));
    let window_id = mgr.get(frame_id).unwrap().selected_window;
    let displayed = Value::string("displayed tab line");
    mgr.get_mut(frame_id)
        .unwrap()
        .prepare_live_window_presentation(
            geometry::PresentationId::new(9),
            vec![WindowDisplaySnapshot {
                window_id,
                chrome_strings: vec![PresentedWindowChromeString::new(
                    PresentedWindowChromeArea::TabLine,
                    neomacs_display_protocol::GlyphStringId::new(1),
                    displayed,
                )],
                ..Default::default()
            }],
        )
        .unwrap();

    let mut roots = Vec::new();
    mgr.trace_roots(&mut roots);
    assert!(roots.contains(&displayed));

    mgr.get_mut(frame_id)
        .unwrap()
        .activate_display_presentation(geometry::PresentationId::new(9))
        .unwrap();
    roots.clear();
    mgr.trace_roots(&mut roots);
    assert!(roots.contains(&displayed));
}

#[test]
fn default_gui_tool_bar_line_height_uses_gnu_image_margin_relief_model() {
    assert_eq!(default_gui_tool_bar_line_height(14.0), 34);
    assert_eq!(default_gui_tool_bar_line_height(28.0), 68);
    assert_eq!(default_gui_tool_bar_line_height(f32::NAN), 34);
}

#[test]
fn sync_tool_bar_height_uses_scaled_gnu_pixel_height_for_gui_frames() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let frame = mgr.get_mut(fid).unwrap();

    frame.set_window_system(Some(Value::symbol("neo")));
    frame.font_pixel_size = 28.0;
    frame.char_height = 33.0;
    frame.set_parameter(Value::symbol("tool-bar-lines"), Value::fixnum(2));
    frame.sync_tool_bar_height_from_parameters();

    assert_eq!(frame.tool_bar_height, 136);
}

#[test]
fn sync_tool_bar_height_keeps_tty_line_height_contract() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let frame = mgr.get_mut(fid).unwrap();

    frame.font_pixel_size = 28.0;
    frame.char_height = 33.0;
    frame.set_parameter(Value::symbol("tool-bar-lines"), Value::fixnum(2));
    frame.sync_tool_bar_height_from_parameters();

    assert_eq!(frame.tool_bar_height, 66);
}

#[test]
fn split_window_horizontal() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];

    let new_wid = mgr.split_window(
        fid,
        wid,
        SplitDirection::Horizontal,
        BufferId(2),
        None,
        SplitPlacement::AfterTarget,
    );
    assert!(new_wid.is_some());

    let frame = mgr.get(fid).unwrap();
    assert_eq!(frame.window_count(), 2);
}

#[test]
fn window_tree_path_rejects_a_different_topology_generation() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];
    let path = mgr
        .leaf_window_paths(fid)
        .expect("frame paths")
        .into_iter()
        .find_map(|(id, path)| (id == wid).then_some(path))
        .expect("selected leaf path");

    mgr.split_window(
        fid,
        wid,
        SplitDirection::Horizontal,
        BufferId(2),
        None,
        SplitPlacement::AfterTarget,
    )
    .expect("split window");

    assert!(
        mgr.frame_and_window_at_path(fid, &path).is_none(),
        "a structurally valid-looking old route must be rejected before any live publication"
    );
}

#[test]
fn split_window_vertical() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];

    let new_wid = mgr.split_window(
        fid,
        wid,
        SplitDirection::Vertical,
        BufferId(2),
        None,
        SplitPlacement::AfterTarget,
    );
    assert!(new_wid.is_some());

    let frame = mgr.get(fid).unwrap();
    assert_eq!(frame.window_count(), 2);
}

#[test]
fn split_window_copies_window_display_state() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    {
        let frame = mgr.get_mut(fid).unwrap();
        frame.set_window_system(Some(Value::symbol("neo")));
        let wid = frame.window_list()[0];
        let display = frame
            .find_window_mut(wid)
            .and_then(Window::display_mut)
            .expect("leaf display");
        display.display_table = Value::fixnum(17);
        display.cursor_type = Value::NIL;
        display.left_fringe_width = 3;
        display.right_fringe_width = 5;
        display.fringes_outside_margins = true;
        display.fringes_persistent = true;
        display.scroll_bar_width = 11;
        display.vertical_scroll_bar_type = Value::T;
        display.scroll_bar_height = 7;
        display.horizontal_scroll_bar_type = Value::NIL;
        display.scroll_bars_persistent = true;
    }

    let original_wid = mgr.get(fid).unwrap().window_list()[0];
    let new_wid = mgr
        .split_window(
            fid,
            original_wid,
            SplitDirection::Horizontal,
            BufferId(2),
            None,
            SplitPlacement::AfterTarget,
        )
        .expect("split");

    let frame = mgr.get(fid).unwrap();
    let original_display = frame
        .find_window(original_wid)
        .and_then(Window::display)
        .expect("original display");
    let new_display = frame
        .find_window(new_wid)
        .and_then(Window::display)
        .expect("new display");

    assert_eq!(original_display.display_table, Value::fixnum(17));
    assert_eq!(new_display.display_table, Value::fixnum(17));
    assert_eq!(original_display.cursor_type, Value::NIL);
    assert_eq!(new_display.cursor_type, Value::NIL);
    assert_eq!(original_display.left_fringe_width, 3);
    assert_eq!(new_display.left_fringe_width, 3);
    assert_eq!(original_display.right_fringe_width, 5);
    assert_eq!(new_display.right_fringe_width, 5);
    assert!(original_display.fringes_outside_margins);
    assert!(new_display.fringes_outside_margins);
    assert!(original_display.fringes_persistent);
    assert!(new_display.fringes_persistent);
    assert_eq!(original_display.scroll_bar_width, 11);
    assert_eq!(new_display.scroll_bar_width, 11);
    assert_eq!(original_display.vertical_scroll_bar_type, Value::T);
    assert_eq!(new_display.vertical_scroll_bar_type, Value::T);
    assert_eq!(original_display.scroll_bar_height, 7);
    assert_eq!(new_display.scroll_bar_height, 7);
    assert_eq!(original_display.horizontal_scroll_bar_type, Value::NIL);
    assert_eq!(new_display.horizontal_scroll_bar_type, Value::NIL);
    assert!(original_display.scroll_bars_persistent);
    assert!(new_display.scroll_bars_persistent);
}

#[test]
fn split_window_resets_new_leaf_vscroll_state() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let original_wid = mgr.get(fid).unwrap().window_list()[0];

    if let Some(Window::Leaf {
        vscroll,
        preserve_vscroll_p,
        ..
    }) = mgr
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(original_wid))
    {
        *vscroll = -19;
        *preserve_vscroll_p = true;
    }

    let new_wid = mgr
        .split_window(
            fid,
            original_wid,
            SplitDirection::Horizontal,
            BufferId(2),
            None,
            SplitPlacement::AfterTarget,
        )
        .expect("split");

    let frame = mgr.get(fid).unwrap();
    let Window::Leaf {
        vscroll: original_vscroll,
        preserve_vscroll_p: original_preserve,
        ..
    } = frame.find_window(original_wid).unwrap()
    else {
        panic!("expected original leaf");
    };
    let Window::Leaf {
        vscroll: new_vscroll,
        preserve_vscroll_p: new_preserve,
        ..
    } = frame.find_window(new_wid).unwrap()
    else {
        panic!("expected new leaf");
    };

    assert_eq!(*original_vscroll, -19);
    assert!(*original_preserve);
    assert_eq!(*new_vscroll, 0);
    assert!(!*new_preserve);
}

#[test]
fn delete_window() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];

    // Split first.
    let new_wid = mgr
        .split_window(
            fid,
            wid,
            SplitDirection::Horizontal,
            BufferId(2),
            None,
            SplitPlacement::AfterTarget,
        )
        .unwrap();

    // Delete the new window.
    assert!(mgr.delete_window(fid, new_wid));
    assert_eq!(mgr.get(fid).unwrap().window_count(), 1);
}

#[test]
fn cannot_delete_last_window() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];

    assert!(!mgr.delete_window(fid, wid));
}

#[test]
fn select_window() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];

    let new_wid = mgr
        .split_window(
            fid,
            wid,
            SplitDirection::Horizontal,
            BufferId(2),
            None,
            SplitPlacement::AfterTarget,
        )
        .unwrap();

    assert!(mgr.get_mut(fid).unwrap().select_window(new_wid));
    assert_eq!(mgr.get(fid).unwrap().selected_window.0, new_wid.0,);
}

#[test]
fn window_at_coordinates() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];

    mgr.split_window(
        fid,
        wid,
        SplitDirection::Horizontal,
        BufferId(2),
        None,
        SplitPlacement::AfterTarget,
    );

    let frame = mgr.get(fid).unwrap();
    // Left half
    let left = frame.window_at(100.0, 300.0);
    assert!(left.is_some());
    // Right half
    let right = frame.window_at(600.0, 300.0);
    assert!(right.is_some());
    // Should be different windows
    assert_ne!(left, right);
}

#[test]
fn frame_columns_and_lines() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let frame = mgr.get(fid).unwrap();

    assert_eq!(frame.columns(), 100); // 800/8
    assert_eq!(frame.lines(), 37); // 600/16 = 37
}

#[test]
fn delete_frame() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    assert_eq!(
        mgr.delete_frame(fid),
        FrameDeletion::Deleted {
            selected: SelectedFrameAfterDeletion::Cleared,
        }
    );
    assert!(mgr.get(fid).is_none());
    assert_eq!(mgr.delete_frame(fid), FrameDeletion::NotFound);
}

#[test]
fn frame_deletion_replacement_policy_distinguishes_mru_from_frame_list_order() {
    let mut mgr = FrameManager::new();
    let oldest = mgr.create_frame("oldest", 80, 25, BufferId(1));
    let middle = mgr.create_frame("middle", 80, 25, BufferId(2));
    let newest = mgr.create_frame("newest", 80, 25, BufferId(3));
    let oldest_window = mgr.get(oldest).expect("oldest frame").selected_window;

    mgr.note_window_selected(oldest_window);
    assert!(mgr.select_frame(newest));

    assert_eq!(
        mgr.replacement_frame_for_deletion(newest, FrameDeletionSelectionPolicy::MostRecentlyUsed,),
        Some(oldest)
    );
    assert_eq!(
        mgr.replacement_frame_for_deletion(newest, FrameDeletionSelectionPolicy::FrameListOrder,),
        Some(middle)
    );
}

#[test]
fn navigation_intents_are_scoped_and_acknowledged_by_identity() {
    let mut manager = FrameManager::new();
    let frame_id = manager.create_frame("intent-frame", 80, 24, BufferId(1));
    let window_id = manager.get(frame_id).expect("frame").selected_window;

    let first_window_intent =
        manager.record_window_navigation_intent(window_id, TransitionDirection::Backward);
    let frame_intent =
        manager.record_frame_navigation_intent(frame_id, TransitionDirection::Forward);

    assert_eq!(
        manager.pending_window_navigation_intent(window_id),
        Some(first_window_intent)
    );
    assert_eq!(
        manager.pending_frame_navigation_intent(frame_id),
        Some(frame_intent)
    );

    let newer_same_direction =
        manager.record_window_navigation_intent(window_id, TransitionDirection::Backward);
    assert_ne!(first_window_intent, newer_same_direction);
    manager.acknowledge_window_navigation_intent(window_id, first_window_intent);
    assert_eq!(
        manager.pending_window_navigation_intent(window_id),
        Some(newer_same_direction),
        "an old presentation must not consume newer same-direction intent"
    );

    manager.acknowledge_window_navigation_intent(window_id, newer_same_direction);
    manager.acknowledge_frame_navigation_intent(frame_id, frame_intent);
    assert_eq!(manager.pending_window_navigation_intent(window_id), None);
    assert_eq!(manager.pending_frame_navigation_intent(frame_id), None);
}

#[test]
fn multiple_frames() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let f1 = mgr.create_frame("F1", 800, 600, BufferId(1));
    let f2 = mgr.create_frame("F2", 1024, 768, BufferId(2));

    assert_eq!(mgr.frame_list().len(), 2);
    assert!(mgr.select_frame(f2));
    assert_eq!(mgr.selected_frame().unwrap().id, f2);

    assert!(mgr.delete_frame(f1).was_deleted());
    assert_eq!(mgr.frame_list().len(), 1);
}

#[test]
fn terminal_top_frame_survives_selecting_a_frame_on_another_terminal() {
    let mut mgr = FrameManager::new();
    let gui = mgr.create_frame_on_terminal("GUI", 0, 800, 600, BufferId(1));
    let first_tty = mgr.create_frame_on_terminal("TTY-1", 7, 80, 25, BufferId(2));
    let second_tty = mgr.create_frame_on_terminal("TTY-2", 7, 80, 25, BufferId(3));

    assert_eq!(mgr.top_frame_on_terminal(7), Some(first_tty));
    assert!(mgr.select_frame(second_tty));
    assert_eq!(mgr.top_frame_on_terminal(7), Some(second_tty));

    assert!(mgr.select_frame(gui));
    assert_eq!(
        mgr.top_frame_on_terminal(7),
        Some(second_tty),
        "another terminal's selection must not erase this TTY's top frame"
    );

    assert!(mgr.delete_frame(second_tty).was_deleted());
    assert_eq!(mgr.top_frame_on_terminal(7), Some(first_tty));
}

#[test]
fn select_frame_retargets_focus_redirections_from_previous_selection() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let selected = mgr.create_frame("F1", 800, 600, BufferId(1));
    let redirected_to_selected = mgr.create_frame("F2", 800, 600, BufferId(2));
    let untouched = mgr.create_frame("F3", 800, 600, BufferId(3));

    mgr.get_mut(redirected_to_selected).unwrap().focus_frame = Value::make_frame(selected.0);
    mgr.get_mut(untouched).unwrap().focus_frame = Value::NIL;

    assert!(mgr.select_frame(redirected_to_selected));
    assert_eq!(
        mgr.get(redirected_to_selected).unwrap().focus_frame_value(),
        Value::make_frame(redirected_to_selected.0)
    );
    assert_eq!(mgr.get(untouched).unwrap().focus_frame_value(), Value::NIL);
}

#[test]
fn rect_contains() {
    crate::test_utils::init_test_tracing();
    let r = Rect::new(10.0, 20.0, 100.0, 50.0);
    assert!(r.contains(10.0, 20.0));
    assert!(r.contains(50.0, 40.0));
    assert!(!r.contains(9.0, 20.0));
    assert!(!r.contains(110.0, 70.0));
}

#[test]
fn find_window_frame_id() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];

    assert_eq!(mgr.find_window_frame_id(wid), Some(fid));
    assert_eq!(mgr.find_window_frame_id(WindowId(99999)), None);
}

#[test]
fn is_live_window_id() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];

    assert!(mgr.is_live_window_id(wid));
    assert!(!mgr.is_live_window_id(WindowId(99999)));
}

#[test]
fn window_parameters() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];

    let key = Value::symbol("my-param");
    let val = Value::fixnum(42);

    // Initially no parameter
    assert!(mgr.window_parameter(wid, &key).is_none());

    mgr.set_window_parameter(wid, key, val);
    assert_eq!(mgr.window_parameter(wid, &key), Some(Value::fixnum(42)));
}

#[test]
fn layout_freshness_types_late_chrome_cache_writes_separately_from_body_mutations() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("chrome-freshness", 800, 600, buffer_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let before = eval
        .window_layout_attempt_freshness(frame_id, window_id, buffer_id)
        .expect("initial freshness");

    eval.frame_manager_mut().set_window_parameter(
        window_id,
        Value::symbol("tab-line-buffers"),
        Value::list(vec![Value::make_buffer(buffer_id)]),
    );
    eval.obarray
        .set_symbol_function("neomacs-test-chrome-lazy-load", Value::T);
    let after_chrome_cache = eval
        .window_layout_attempt_freshness(frame_id, window_id, buffer_id)
        .expect("freshness after chrome cache write");

    assert!(
        !before.remains_valid_across(after_chrome_cache, WindowLayoutLispBoundary::BufferBody,)
    );
    assert!(
        before.remains_valid_across(after_chrome_cache, WindowLayoutLispBoundary::WindowChrome,)
    );

    eval.buffer_manager_mut()
        .get_mut(buffer_id)
        .expect("buffer")
        .insert("body changed");
    let after_body_mutation = eval
        .window_layout_attempt_freshness(frame_id, window_id, buffer_id)
        .expect("freshness after body mutation");
    assert!(
        !after_chrome_cache
            .remains_valid_across(after_body_mutation, WindowLayoutLispBoundary::WindowChrome,)
    );
}

#[test]
fn split_window_does_not_copy_window_parameters() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];
    let key = Value::symbol("my-param");

    mgr.set_window_parameter(wid, key, Value::fixnum(42));
    let new_wid = mgr
        .split_window(
            fid,
            wid,
            SplitDirection::Horizontal,
            BufferId(2),
            None,
            SplitPlacement::AfterTarget,
        )
        .expect("split");

    assert_eq!(mgr.window_parameter(wid, &key), Some(Value::fixnum(42)));
    assert_eq!(mgr.window_parameter(new_wid, &key), None);
}

#[test]
fn deleted_window_retains_window_parameters() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];
    let other = mgr
        .split_window(
            fid,
            wid,
            SplitDirection::Horizontal,
            BufferId(2),
            None,
            SplitPlacement::AfterTarget,
        )
        .expect("split");
    let key = Value::symbol("deleted-param");

    mgr.set_window_parameter(other, key, Value::fixnum(7));
    assert!(mgr.delete_window(fid, other));
    assert_eq!(mgr.window_parameter(other, &key), Some(Value::fixnum(7)));
}

#[test]
fn replace_buffer_in_windows() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];

    // Window should show buffer 1
    let frame = mgr.get(fid).unwrap();
    assert_eq!(
        frame.find_window(wid).unwrap().buffer_id(),
        Some(BufferId(1))
    );

    // Replace buffer 1 with buffer 2
    mgr.replace_buffer_in_windows(BufferId(1), BufferId(2));

    let frame = mgr.get(fid).unwrap();
    assert_eq!(
        frame.find_window(wid).unwrap().buffer_id(),
        Some(BufferId(2))
    );
}

#[test]
fn deep_split_and_delete() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let w1 = mgr.get(fid).unwrap().window_list()[0];

    // Split w1 horizontally → w2
    let w2 = mgr
        .split_window(
            fid,
            w1,
            SplitDirection::Horizontal,
            BufferId(2),
            None,
            SplitPlacement::AfterTarget,
        )
        .unwrap();

    // Split w2 vertically → w3
    let w3 = mgr
        .split_window(
            fid,
            w2,
            SplitDirection::Vertical,
            BufferId(3),
            None,
            SplitPlacement::AfterTarget,
        )
        .unwrap();

    assert_eq!(mgr.get(fid).unwrap().window_count(), 3);

    // Delete w3
    assert!(mgr.delete_window(fid, w3));
    assert_eq!(mgr.get(fid).unwrap().window_count(), 2);

    // Delete w2
    assert!(mgr.delete_window(fid, w2));
    assert_eq!(mgr.get(fid).unwrap().window_count(), 1);

    // w1 is the last one, can't delete
    assert!(!mgr.delete_window(fid, w1));
}

#[test]
fn note_window_selected_updates_use_time() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let w1 = mgr.get(fid).unwrap().window_list()[0];
    let w2 = mgr
        .split_window(
            fid,
            w1,
            SplitDirection::Horizontal,
            BufferId(2),
            None,
            SplitPlacement::AfterTarget,
        )
        .unwrap();

    let t1 = mgr.note_window_selected(w1);
    let t2 = mgr.note_window_selected(w2);
    // Each selection should get a monotonically increasing use-time
    assert!(t2 > t1);
}

#[test]
fn window_set_buffer_resets_position() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().window_list()[0];

    // Modify point
    let frame = mgr.get_mut(fid).unwrap();
    if let Some(w) = frame.find_window_mut(wid) {
        if let Window::Leaf { point, .. } = w {
            *point = LispCharPos1::from_one_based_usize(100);
        }
    }

    // Set buffer resets point to 1
    let frame = mgr.get_mut(fid).unwrap();
    if let Some(w) = frame.find_window_mut(wid) {
        w.set_buffer(BufferId(2));
    }

    let frame = mgr.get(fid).unwrap();
    let w = frame.find_window(wid).unwrap();
    if let Window::Leaf {
        point, buffer_id, ..
    } = w
    {
        assert_eq!(*buffer_id, BufferId(2));
        assert_eq!(*point, LispCharPos1::ONE);
    }
}

#[test]
fn window_start_marker_stays_before_insert_at_start() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let buffer_id = buffers.create_buffer("bob-test");
    buffers
        .insert_into_buffer(buffer_id, "line A\nline B\n")
        .expect("insert test text");
    buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(0))
        .expect("move point to beginning");

    let mut frames = FrameManager::new();
    let frame_id = frames.create_frame("F1", 800, 600, buffer_id);
    let selected_window = frames.get(frame_id).expect("frame").selected_window;
    {
        let frame = frames.get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        crate::window::window_markers::attach_window_position_markers(&mut buffers, window);
    }

    buffers
        .insert_into_buffer(buffer_id, "X")
        .expect("insert before window start");
    crate::window::window_markers::sync_window_positions_from_markers(
        frames.get_mut(frame_id).expect("frame"),
        &buffers,
        buffer_id,
    );

    let frame = frames.get(frame_id).expect("frame");
    let Window::Leaf { window_start, .. } = frame.find_window(selected_window).expect("window")
    else {
        panic!("selected window should be a leaf");
    };
    assert_eq!(*window_start, LispCharPos1::ONE);
}

#[test]
fn frame_resize_pixelwise_updates_window_tree_and_invalidates_display_state() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let w1 = mgr.get(fid).unwrap().window_list()[0];
    let w2 = mgr
        .split_window(
            fid,
            w1,
            SplitDirection::Horizontal,
            BufferId(2),
            None,
            SplitPlacement::AfterTarget,
        )
        .unwrap();

    let frame = mgr.get_mut(fid).unwrap();
    frame.char_width = 10.0;
    frame.char_height = 20.0;
    frame.commit_redisplay_cache_for_test(vec![WindowDisplaySnapshot {
        window_id: w1,
        phys_cursor: Some(WindowCursorSnapshot {
            kind: WindowCursorKind::Bar,
            x: 7,
            y: 13,
            width: 9,
            height: 17,
            ascent: 12,
            row: 2,
            col: 5,
        }),
        ..WindowDisplaySnapshot::default()
    }]);

    frame
        .find_window_mut(w1)
        .unwrap()
        .set_window_end_from_positions(
            LispCharPos1::from_one_based_usize(200),
            EmacsBytePos::new(200),
            LispCharPos1::from_one_based_usize(50),
            EmacsBytePos::new(50),
            3,
        );
    frame
        .find_window_mut(w2)
        .unwrap()
        .set_window_end_from_positions(
            LispCharPos1::from_one_based_usize(200),
            EmacsBytePos::new(200),
            LispCharPos1::from_one_based_usize(60),
            EmacsBytePos::new(60),
            3,
        );

    frame.resize_pixelwise(400, 260);

    assert_eq!(frame.width, 400);
    assert_eq!(frame.height, 260);
    assert!(frame.redisplay_snapshot(w1).is_none());
    assert!(frame.redisplay_snapshot(w2).is_none());
    assert!(
        frame
            .find_window(w1)
            .and_then(|window| window.display())
            .and_then(|display| display.phys_cursor.as_ref())
            .is_none()
    );
    assert_eq!(frame.parameter("width"), Some(Value::fixnum(40)));
    assert_eq!(frame.parameter("height"), Some(Value::fixnum(13)));

    let root_bounds = *frame.root_window.bounds();
    assert_eq!(root_bounds, Rect::new(0.0, 0.0, 400.0, 244.0));

    let mini_bounds = *frame.minibuffer_leaf.as_ref().unwrap().bounds();
    assert_eq!(mini_bounds, Rect::new(0.0, 244.0, 400.0, 16.0));

    assert_eq!(
        frame.find_window(w1).unwrap().bounds(),
        &Rect::new(0.0, 0.0, 200.0, 244.0)
    );
    assert_eq!(
        frame.find_window(w2).unwrap().bounds(),
        &Rect::new(200.0, 0.0, 200.0, 244.0)
    );
    assert_eq!(
        frame.find_window(w1).unwrap().window_end_valid(),
        Some(false)
    );
    assert_eq!(
        frame.find_window(w2).unwrap().window_end_valid(),
        Some(false)
    );
    assert_eq!(
        frame.minibuffer_leaf.as_ref().unwrap().window_end_valid(),
        Some(false)
    );
}

#[test]
fn resize_pixelwise_minibuffer_only_does_not_add_minibuffer_line() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("mini", 100, 60, BufferId(1));
    let frame = mgr.get_mut(fid).expect("frame");
    frame.set_window_system(Some(Value::symbol("x")));
    frame.char_height = 15.0;
    let root_window_id = frame.root_window.id();
    frame.minibuffer_leaf = None;
    frame.minibuffer_window = Some(root_window_id);

    frame.resize_pixelwise(100, 60);

    assert_eq!(frame.parameter("height"), Some(Value::fixnum(4)));
    assert_eq!(
        frame.parameter("neovm--frame-text-lines"),
        Some(Value::fixnum(4))
    );
}

#[test]
fn frame_resize_discards_geometry_dependent_auto_hscroll() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("posframe", 9, 19, BufferId(1));
    let frame = mgr.get_mut(fid).expect("frame");
    let window = frame
        .find_window_mut(frame.selected_window)
        .expect("selected window");
    let Window::Leaf {
        hscroll,
        min_hscroll,
        suspend_auto_hscroll,
        ..
    } = window
    else {
        panic!("selected window should be a leaf");
    };
    // A current-line auto-hscroll pass ran while the child frame was only one
    // column wide.  The offset is derived from that geometry, not user intent.
    *hscroll = 15;
    *min_hscroll = 0;
    *suspend_auto_hscroll = false;

    frame.resize_pixelwise(1_955, 376);

    assert_eq!(
        frame
            .find_window(frame.selected_window)
            .expect("selected window")
            .hscroll(),
        0,
        "a resized child frame must not present an auto-hscroll offset computed for its bootstrap size"
    );
}

#[test]
fn frame_resize_preserves_manual_hscroll() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("manual-hscroll", 800, 600, BufferId(1));
    let frame = mgr.get_mut(fid).expect("frame");
    let window = frame
        .find_window_mut(frame.selected_window)
        .expect("selected window");
    let Window::Leaf {
        hscroll,
        min_hscroll,
        suspend_auto_hscroll,
        ..
    } = window
    else {
        panic!("selected window should be a leaf");
    };
    // Mirrors `(set-window-hscroll WINDOW 12)`: GNU marks this state as
    // suspended so redisplay does not reinterpret an explicit user offset.
    *hscroll = 12;
    // Keep the automatic lower bound distinct: resetting every window to
    // min_hscroll would erase the explicit offset and must fail this test.
    *min_hscroll = 0;
    *suspend_auto_hscroll = true;

    frame.resize_pixelwise(1_200, 700);

    assert_eq!(
        frame
            .find_window(frame.selected_window)
            .expect("selected window")
            .hscroll(),
        12
    );
}

#[test]
fn frame_resize_pixelwise_preserves_fixed_width_side_window() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 400, 260, BufferId(1));
    let frame = mgr.get_mut(fid).unwrap();
    frame.char_width = 10.0;
    frame.char_height = 20.0;
    let w1 = mgr.get(fid).unwrap().window_list()[0];
    let side = mgr
        .split_window(
            fid,
            w1,
            SplitDirection::Horizontal,
            BufferId(2),
            Some(200),
            SplitPlacement::BeforeTarget,
        )
        .unwrap();

    let frame = mgr.get_mut(fid).unwrap();
    frame
        .find_window_mut(side)
        .unwrap()
        .set_fixed_width_cols(20);

    frame.resize_pixelwise(800, 260);

    assert_eq!(
        frame.find_window(side).unwrap().bounds(),
        &Rect::new(0.0, 0.0, 200.0, 244.0)
    );
    assert_eq!(
        frame.find_window(w1).unwrap().bounds(),
        &Rect::new(200.0, 0.0, 600.0, 244.0)
    );
}

#[test]
fn frame_resize_pixelwise_preserves_flexible_window_proportions() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let frame = mgr.get_mut(fid).unwrap();
    frame.char_width = 10.0;
    frame.char_height = 20.0;
    let main = mgr.get(fid).unwrap().window_list()[0];
    let side = mgr
        .split_window(
            fid,
            main,
            SplitDirection::Horizontal,
            BufferId(2),
            Some(200),
            SplitPlacement::BeforeTarget,
        )
        .unwrap();

    let frame = mgr.get_mut(fid).unwrap();
    frame.resize_pixelwise(800, 260);

    assert_eq!(
        frame.find_window(side).unwrap().bounds(),
        &Rect::new(0.0, 0.0, 200.0, 244.0)
    );
    assert_eq!(
        frame.find_window(main).unwrap().bounds(),
        &Rect::new(200.0, 0.0, 600.0, 244.0)
    );
}

#[test]
fn completed_redisplay_syncs_live_window_cursor_state() {
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().selected_window;
    let cursor = WindowCursorSnapshot {
        kind: WindowCursorKind::Bar,
        x: 11,
        y: 29,
        width: 3,
        height: 16,
        ascent: 12,
        row: 1,
        col: 4,
    };
    let output_cursor = WindowCursorPos {
        x: 44,
        y: 29,
        row: 1,
        col: 8,
    };

    let frame = mgr.get_mut(fid).unwrap();
    frame.commit_redisplay_cache_for_test(vec![WindowDisplaySnapshot {
        window_id: wid,
        phys_cursor: Some(cursor.clone()),
        rows: vec![DisplayRowSnapshot {
            row: 1,
            y: 29,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: output_cursor.x,
            end_col: output_cursor.col,
            start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
            end_buffer_pos: Some(crate::buffer::LispCharPos1::new(8)),
            fringe: Default::default(),
        }],
        ..WindowDisplaySnapshot::default()
    }]);

    let display = frame
        .find_window(wid)
        .and_then(|window| window.display())
        .expect("window display state");
    let cursor_pos = WindowCursorPos::from_snapshot(&cursor);
    assert!(display.phys_cursor_on_p);
    assert_eq!(display.phys_cursor_type, WindowCursorKind::Bar);
    assert!(!display.last_cursor_off_p);
    assert_eq!(display.last_cursor_vpos, cursor.row);
    assert_eq!(display.cursor.as_ref(), Some(&cursor_pos));
    assert_eq!(display.phys_cursor.as_ref(), Some(&cursor));
    assert_eq!(display.output_cursor.as_ref(), Some(&output_cursor));
}

#[test]
fn completed_redisplay_replaces_old_output_cursor_progress() {
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().selected_window;
    let frame = mgr.get_mut(fid).unwrap();

    frame.commit_redisplay_cache_for_test(vec![WindowDisplaySnapshot {
        window_id: wid,
        rows: vec![DisplayRowSnapshot {
            row: 1,
            y: 29,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 44,
            end_col: 8,
            start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
            end_buffer_pos: Some(crate::buffer::LispCharPos1::new(8)),
            fringe: Default::default(),
        }],
        ..WindowDisplaySnapshot::default()
    }]);

    frame.commit_redisplay_cache_for_test(vec![WindowDisplaySnapshot {
        window_id: wid,
        rows: vec![DisplayRowSnapshot {
            row: 3,
            y: 61,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 88,
            end_col: 12,
            start_buffer_pos: Some(crate::buffer::LispCharPos1::new(20)),
            end_buffer_pos: Some(crate::buffer::LispCharPos1::new(32)),
            fringe: Default::default(),
        }],
        ..WindowDisplaySnapshot::default()
    }]);

    let display = frame
        .find_window(wid)
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(
        display.output_cursor,
        Some(WindowCursorPos {
            x: 88,
            y: 61,
            row: 3,
            col: 12,
        })
    );
}

#[test]
fn cache_only_fixture_preserves_live_window_cursor_state() {
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().selected_window;
    let cursor = WindowCursorSnapshot {
        kind: WindowCursorKind::Bar,
        x: 11,
        y: 29,
        width: 3,
        height: 16,
        ascent: 12,
        row: 1,
        col: 4,
    };
    let cursor_pos = WindowCursorPos::from_snapshot(&cursor);
    let output_cursor = WindowCursorPos {
        x: 44,
        y: 29,
        row: 1,
        col: 8,
    };
    let snapshot = WindowDisplaySnapshot {
        window_id: wid,
        phys_cursor: Some(cursor.clone()),
        rows: vec![DisplayRowSnapshot {
            row: 1,
            y: 29,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: output_cursor.x,
            end_col: output_cursor.col,
            start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
            end_buffer_pos: Some(crate::buffer::LispCharPos1::new(8)),
            fringe: Default::default(),
        }],
        ..WindowDisplaySnapshot::default()
    };

    let frame = mgr.get_mut(fid).unwrap();
    frame.begin_display_output_pass();
    frame.replay_window_output_snapshot(&snapshot);
    frame.replace_redisplay_cache_for_test(vec![WindowDisplaySnapshot {
        window_id: wid,
        phys_cursor: None,
        ..WindowDisplaySnapshot::default()
    }]);

    let display = frame
        .find_window(wid)
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(display.cursor.as_ref(), Some(&cursor_pos));
    assert_eq!(display.output_cursor.as_ref(), Some(&output_cursor));
    assert_eq!(display.phys_cursor.as_ref(), Some(&cursor));
    assert_eq!(
        frame
            .redisplay_snapshot(wid)
            .and_then(|snapshot| snapshot.phys_cursor.as_ref()),
        None
    );
}

#[test]
fn no_op_set_window_vscroll_preserves_display_snapshot() {
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().selected_window;
    {
        let frame = mgr.get_mut(fid).unwrap();
        frame.set_window_system(Some(Value::symbol("x")));
        frame.replace_redisplay_cache_for_test(vec![WindowDisplaySnapshot {
            window_id: wid,
            points: vec![DisplayPointSnapshot {
                role: DisplayPointRole::Glyph,
                buffer_pos: crate::buffer::LispCharPos1::new(5),
                x: 64,
                y: 0,
                width: 16,
                height: 33,
                row: 0,
                col: 4,
            }],
            rows: vec![DisplayRowSnapshot {
                row: 0,
                y: 0,
                height: 33,
                start_x: 0,
                start_col: 0,
                end_x: 96,
                end_col: 6,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(6)),
                fringe: Default::default(),
            }],
            ..WindowDisplaySnapshot::default()
        }]);
    }

    let returned = mgr.set_window_vscroll(wid, 0.0, true, true);

    assert_eq!(returned, Some(Value::fixnum(0)));
    assert!(
        mgr.get(fid)
            .and_then(|frame| frame.redisplay_snapshot(wid))
            .and_then(|snapshot| {
                snapshot.point_for_buffer_pos(crate::buffer::LispCharPos1::new(5))
            })
            .is_some(),
        "GNU Fset_window_vscroll only invalidates redisplay when vscroll changes"
    );
}

#[test]
fn completed_redisplay_preserves_logical_cursor_without_physical_cursor() {
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().selected_window;
    let logical_cursor = WindowCursorPos {
        x: 24,
        y: 16,
        row: 1,
        col: 3,
    };

    let frame = mgr.get_mut(fid).unwrap();
    frame.commit_redisplay_cache_for_test(vec![WindowDisplaySnapshot {
        window_id: wid,
        logical_cursor: Some(logical_cursor),
        rows: vec![DisplayRowSnapshot {
            row: 1,
            y: 16,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 64,
            end_col: 8,
            start_buffer_pos: Some(crate::buffer::LispCharPos1::new(10)),
            end_buffer_pos: Some(crate::buffer::LispCharPos1::new(18)),
            fringe: Default::default(),
        }],
        ..WindowDisplaySnapshot::default()
    }]);

    let display = frame
        .find_window(wid)
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(display.cursor, Some(logical_cursor));
    assert_eq!(
        display.output_cursor,
        Some(WindowCursorPos {
            x: 64,
            y: 16,
            row: 1,
            col: 8,
        })
    );
    assert_eq!(display.phys_cursor, None);
    assert_eq!(display.phys_cursor_type, WindowCursorKind::NoCursor);
    assert!(!display.phys_cursor_on_p);
}

#[test]
fn completed_redisplay_commits_last_cursor_visibility_state() {
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().selected_window;
    let cursor = WindowCursorSnapshot {
        kind: WindowCursorKind::FilledBox,
        x: 4,
        y: 8,
        width: 8,
        height: 16,
        ascent: 12,
        row: 0,
        col: 0,
    };

    let frame = mgr.get_mut(fid).unwrap();
    let display = frame
        .find_window_mut(wid)
        .and_then(|window| window.display_mut())
        .expect("window display state");
    display.cursor_off_p = true;

    frame.commit_redisplay_cache_for_test(vec![WindowDisplaySnapshot {
        window_id: wid,
        phys_cursor: Some(cursor),
        ..WindowDisplaySnapshot::default()
    }]);

    let display = frame
        .find_window(wid)
        .and_then(|window| window.display())
        .expect("window display state");
    assert!(display.cursor_off_p);
    assert!(display.last_cursor_off_p);
}

#[test]
fn clear_physical_cursor_state_preserves_committed_cursor_history() {
    let cursor = WindowCursorSnapshot {
        kind: WindowCursorKind::FilledBox,
        x: 9,
        y: 21,
        width: 8,
        height: 16,
        ascent: 12,
        row: 2,
        col: 5,
    };
    let snapshot = WindowDisplaySnapshot {
        window_id: WindowId(1),
        phys_cursor: Some(cursor.clone()),
        rows: vec![DisplayRowSnapshot {
            row: 2,
            y: 21,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 9,
            end_col: 5,
            start_buffer_pos: Some(crate::buffer::LispCharPos1::new(11)),
            end_buffer_pos: Some(crate::buffer::LispCharPos1::new(11)),
            fringe: Default::default(),
        }],
        ..WindowDisplaySnapshot::default()
    };
    let mut display = WindowDisplayState::default();
    display.begin_output_pass();
    display.install_logical_cursor(Some(WindowCursorPos::from_snapshot(&cursor)));
    {
        let mut update = WindowOutputUpdate::new(&mut display);
        update.replay_output_rows(&snapshot.rows);
    }
    display.apply_physical_cursor_snapshot(Some(cursor.clone()));
    display.commit_completed_redisplay();

    display.clear_physical_cursor_state();

    let cursor_pos = WindowCursorPos::from_snapshot(&cursor);
    assert_eq!(display.cursor, Some(cursor_pos));
    assert_eq!(display.output_cursor, Some(cursor_pos));
    assert_eq!(display.phys_cursor, None);
    assert_eq!(display.phys_cursor_type, WindowCursorKind::NoCursor);
    assert!(!display.phys_cursor_on_p);
    assert!(!display.last_cursor_off_p);
    assert_eq!(display.last_cursor_vpos, cursor.row);
}

#[test]
fn begin_output_pass_preserves_committed_output_cursor_until_next_commit() {
    let cursor = WindowCursorSnapshot {
        kind: WindowCursorKind::FilledBox,
        x: 9,
        y: 21,
        width: 8,
        height: 16,
        ascent: 12,
        row: 2,
        col: 5,
    };
    let cursor_pos = WindowCursorPos::from_snapshot(&cursor);
    let mut display = WindowDisplayState::default();

    display.install_logical_cursor(Some(cursor_pos));
    display.output_cursor_to(cursor_pos);
    display.apply_physical_cursor_snapshot(Some(cursor.clone()));

    display.begin_output_pass();

    assert_eq!(display.cursor, None);
    assert_eq!(display.output_cursor, Some(cursor_pos));
    assert_eq!(display.phys_cursor, None);
    assert_eq!(display.phys_cursor_type, WindowCursorKind::NoCursor);
    assert!(!display.phys_cursor_on_p);
}

#[test]
fn begin_window_output_update_clears_output_cursor_for_active_window() {
    let cursor = WindowCursorSnapshot {
        kind: WindowCursorKind::FilledBox,
        x: 9,
        y: 21,
        width: 8,
        height: 16,
        ascent: 12,
        row: 2,
        col: 5,
    };
    let cursor_pos = WindowCursorPos::from_snapshot(&cursor);
    let mut display = WindowDisplayState::default();

    display.install_logical_cursor(Some(cursor_pos));
    display.output_cursor_to(cursor_pos);
    display.apply_physical_cursor_snapshot(Some(cursor));

    display.begin_window_output_update();

    assert_eq!(display.cursor, None);
    assert_eq!(display.output_cursor, None);
    assert_eq!(display.phys_cursor, None);
    assert_eq!(display.phys_cursor_type, WindowCursorKind::NoCursor);
    assert!(!display.phys_cursor_on_p);
}

#[test]
fn output_cursor_tracks_explicit_output_lifecycle() {
    let logical_cursor = WindowCursorPos {
        x: 12,
        y: 24,
        row: 1,
        col: 3,
    };
    let mut display = WindowDisplayState::default();

    display.install_logical_cursor(Some(logical_cursor));
    assert_eq!(display.cursor, Some(logical_cursor));
    assert_eq!(display.output_cursor, None);

    display.output_cursor_to(logical_cursor);
    assert_eq!(display.output_cursor, Some(logical_cursor));

    display.output_cursor_to(WindowCursorPos {
        x: 36,
        y: 24,
        row: 1,
        col: 6,
    });
    assert_eq!(
        display.output_cursor,
        Some(WindowCursorPos {
            x: 36,
            y: 24,
            row: 1,
            col: 6,
        })
    );
    assert_eq!(display.cursor, Some(logical_cursor));
}

#[test]
fn completed_redisplay_preserves_point_row_history_over_output_progress() {
    let mut display = WindowDisplayState::default();
    display.install_logical_cursor(Some(WindowCursorPos {
        x: 12,
        y: 24,
        row: 1,
        col: 3,
    }));
    display.output_cursor_to(WindowCursorPos {
        x: 80,
        y: 72,
        row: 4,
        col: 9,
    });

    display.commit_completed_redisplay();

    // GNU xdisp.c commits `w->last_cursor_vpos = w->cursor.vpos`, so live
    // output progress must not override the window's logical cursor row.
    assert_eq!(display.last_cursor_vpos, 1);
}

#[test]
fn output_pass_commits_output_cursor_from_row_geometry() {
    let cursor = WindowCursorSnapshot {
        kind: WindowCursorKind::Bar,
        x: 44,
        y: 32,
        width: 3,
        height: 16,
        ascent: 12,
        row: 2,
        col: 7,
    };
    let snapshot = WindowDisplaySnapshot {
        window_id: WindowId(1),
        phys_cursor: Some(cursor.clone()),
        rows: vec![DisplayRowSnapshot {
            row: 2,
            y: 32,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 80,
            end_col: 12,
            start_buffer_pos: Some(crate::buffer::LispCharPos1::new(20)),
            end_buffer_pos: Some(crate::buffer::LispCharPos1::new(32)),
            fringe: Default::default(),
        }],
        ..WindowDisplaySnapshot::default()
    };
    let mut display = WindowDisplayState::default();

    display.begin_output_pass();
    display.install_logical_cursor(Some(WindowCursorPos::from_snapshot(&cursor)));
    {
        let mut update = WindowOutputUpdate::new(&mut display);
        update.replay_output_rows(&snapshot.rows);
    }
    display.apply_physical_cursor_snapshot(Some(cursor.clone()));
    display.commit_completed_redisplay();

    assert_eq!(
        display.output_cursor,
        Some(WindowCursorPos {
            x: 80,
            y: 32,
            row: 2,
            col: 12,
        })
    );
    assert_eq!(display.last_cursor_vpos, 2);
    assert_eq!(display.phys_cursor, Some(cursor));
}

#[test]
fn explicit_window_output_finalization_prefers_live_output_progress() {
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().selected_window;
    let cursor = WindowCursorSnapshot {
        kind: WindowCursorKind::Bar,
        x: 44,
        y: 32,
        width: 3,
        height: 16,
        ascent: 12,
        row: 2,
        col: 7,
    };
    let frame = mgr.get_mut(fid).expect("frame");
    frame.begin_display_output_pass();
    {
        let mut update = frame.window_output_update(wid).expect("window update");
        update.begin_update();
        update.output_cursor_to_coords(2, 0, 32, 0);
        update.output_cursor_to_coords(2, 12, 32, 80);
        update.finalize_live_update(
            Some(WindowCursorPos::from_snapshot(&cursor)),
            Some(cursor.clone()),
        );
    }

    let display = frame
        .find_window(wid)
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(
        display.output_cursor,
        Some(WindowCursorPos {
            x: 80,
            y: 32,
            row: 2,
            col: 12,
        })
    );
    assert_eq!(display.phys_cursor, Some(cursor));
}

#[test]
fn frame_output_progress_api_tracks_intra_row_progress() {
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().selected_window;
    let frame = mgr.get_mut(fid).expect("frame");

    frame.begin_display_output_pass();
    {
        let mut update = frame.window_output_update(wid).expect("window update");
        update.begin_update();
        update.output_cursor_to_coords(2, 3, 32, 24);
        update.output_cursor_to_coords(2, 7, 32, 56);
    }

    let display = frame
        .find_window(wid)
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(
        display.output_cursor,
        Some(WindowCursorPos {
            x: 56,
            y: 32,
            row: 2,
            col: 7,
        })
    );
}

#[test]
fn explicit_window_output_finalization_preserves_live_logical_and_physical_cursor_state() {
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().selected_window;
    let live_cursor = WindowCursorPos {
        x: 18,
        y: 16,
        row: 1,
        col: 2,
    };
    let live_phys = WindowCursorSnapshot {
        kind: WindowCursorKind::Bar,
        x: 18,
        y: 16,
        width: 3,
        height: 16,
        ascent: 12,
        row: 1,
        col: 2,
    };
    let snapshot_phys = WindowCursorSnapshot {
        kind: WindowCursorKind::FilledBox,
        x: 80,
        y: 64,
        width: 8,
        height: 16,
        ascent: 12,
        row: 4,
        col: 10,
    };
    let snapshot = WindowDisplaySnapshot {
        window_id: wid,
        logical_cursor: Some(WindowCursorPos::from_snapshot(&snapshot_phys)),
        phys_cursor: Some(snapshot_phys),
        rows: vec![DisplayRowSnapshot {
            row: 4,
            y: 64,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 144,
            end_col: 18,
            start_buffer_pos: Some(crate::buffer::LispCharPos1::new(20)),
            end_buffer_pos: Some(crate::buffer::LispCharPos1::new(38)),
            fringe: Default::default(),
        }],
        ..WindowDisplaySnapshot::default()
    };
    let frame = mgr.get_mut(fid).expect("frame");

    frame.begin_display_output_pass();
    {
        let mut update = frame.window_output_update(wid).expect("window update");
        update.begin_update();
        update.finalize_with_output_fallback(Some(live_cursor), Some(live_phys.clone()), &snapshot);
    }

    let display = frame
        .find_window(wid)
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(display.cursor, Some(live_cursor));
    assert_eq!(display.phys_cursor, Some(live_phys));
    assert_eq!(
        display.output_cursor,
        Some(WindowCursorPos {
            x: 144,
            y: 64,
            row: 4,
            col: 18,
        })
    );
}

#[test]
fn finish_window_output_update_preserves_live_cursor_state_with_snapshot_output_fallback() {
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().selected_window;
    let live_cursor = WindowCursorPos {
        x: 18,
        y: 16,
        row: 1,
        col: 2,
    };
    let live_phys = WindowCursorSnapshot {
        kind: WindowCursorKind::Bar,
        x: 18,
        y: 16,
        width: 3,
        height: 16,
        ascent: 12,
        row: 1,
        col: 2,
    };
    let snapshot = WindowDisplaySnapshot {
        window_id: wid,
        rows: vec![DisplayRowSnapshot {
            row: 4,
            y: 64,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 144,
            end_col: 18,
            start_buffer_pos: Some(crate::buffer::LispCharPos1::new(20)),
            end_buffer_pos: Some(crate::buffer::LispCharPos1::new(38)),
            fringe: Default::default(),
        }],
        ..WindowDisplaySnapshot::default()
    };
    let frame = mgr.get_mut(fid).expect("frame");

    frame.begin_display_output_pass();
    {
        let mut update = frame.window_output_update(wid).expect("window update");
        update.begin_update();
        update.finalize_with_output_fallback(Some(live_cursor), Some(live_phys.clone()), &snapshot);
    }

    let display = frame
        .find_window(wid)
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(display.cursor, Some(live_cursor));
    assert_eq!(display.phys_cursor, Some(live_phys));
    assert_eq!(
        display.output_cursor,
        Some(WindowCursorPos {
            x: 144,
            y: 64,
            row: 4,
            col: 18,
        })
    );
    assert_eq!(display.last_cursor_vpos, 1);
}

#[test]
fn output_pass_keeps_cursor_target_and_output_progress_separate() {
    let cursor = WindowCursorSnapshot {
        kind: WindowCursorKind::Bar,
        x: 18,
        y: 16,
        width: 3,
        height: 16,
        ascent: 12,
        row: 1,
        col: 2,
    };
    let snapshot = WindowDisplaySnapshot {
        window_id: WindowId(1),
        phys_cursor: Some(cursor.clone()),
        rows: vec![
            DisplayRowSnapshot {
                row: 0,
                y: 0,
                height: 16,
                start_x: 0,
                start_col: 0,
                end_x: 64,
                end_col: 8,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(8)),
                fringe: Default::default(),
            },
            DisplayRowSnapshot {
                row: 1,
                y: 16,
                height: 16,
                start_x: 0,
                start_col: 0,
                end_x: 72,
                end_col: 9,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(9)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(17)),
                fringe: Default::default(),
            },
            DisplayRowSnapshot {
                row: 2,
                y: 32,
                height: 16,
                start_x: 0,
                start_col: 0,
                end_x: 80,
                end_col: 10,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(18)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(27)),
                fringe: Default::default(),
            },
        ],
        ..WindowDisplaySnapshot::default()
    };
    let mut display = WindowDisplayState::default();

    display.begin_output_pass();
    display.install_logical_cursor(Some(WindowCursorPos::from_snapshot(&cursor)));
    {
        let mut update = WindowOutputUpdate::new(&mut display);
        update.replay_output_rows(&snapshot.rows);
    }
    display.apply_physical_cursor_snapshot(Some(cursor.clone()));
    display.commit_completed_redisplay();

    assert_eq!(
        display.cursor,
        Some(WindowCursorPos {
            x: 18,
            y: 16,
            row: 1,
            col: 2,
        })
    );
    assert_eq!(
        display.output_cursor,
        Some(WindowCursorPos {
            x: 80,
            y: 32,
            row: 2,
            col: 10,
        })
    );
    assert_eq!(display.last_cursor_vpos, 1);
    assert_eq!(display.phys_cursor, Some(cursor));
}

#[test]
fn completed_redisplay_preserves_output_cursor_for_omitted_windows() {
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let wid = mgr.get(fid).unwrap().selected_window;
    let cursor = WindowCursorSnapshot {
        kind: WindowCursorKind::FilledBox,
        x: 18,
        y: 36,
        width: 8,
        height: 16,
        ascent: 12,
        row: 2,
        col: 6,
    };
    let cursor_pos = WindowCursorPos::from_snapshot(&cursor);

    let frame = mgr.get_mut(fid).unwrap();
    let display = frame
        .find_window_mut(wid)
        .and_then(|window| window.display_mut())
        .expect("window display state");
    display.install_logical_cursor(Some(cursor_pos));
    display.output_cursor_to(cursor_pos);
    display.apply_physical_cursor_snapshot(Some(cursor));
    display.commit_completed_redisplay();

    frame.commit_redisplay_cache_for_test(Vec::new());

    let display = frame
        .find_window(wid)
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(display.cursor, None);
    assert_eq!(display.output_cursor, Some(cursor_pos));
    assert_eq!(display.phys_cursor, None);
    assert_eq!(display.phys_cursor_type, WindowCursorKind::NoCursor);
    assert!(!display.phys_cursor_on_p);
    assert_eq!(display.last_cursor_vpos, cursor_pos.row);
}

#[test]
fn frame_resize_pixelwise_reserves_tab_bar_height_above_root_window_tree() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 800, 600, BufferId(1));
    let frame = mgr.get_mut(fid).unwrap();
    frame.char_width = 10.0;
    frame.char_height = 20.0;
    // GNU offsets the tab-bar only on a displayed frame (commit 1030f559b);
    // mark this test frame displayed so the reflow reserves the bar.
    frame.displays_chrome = true;
    frame.set_parameter(Value::symbol("tab-bar-lines"), Value::fixnum(1));

    frame.sync_tab_bar_height_from_parameters();
    frame.resize_pixelwise(400, 260);

    assert_eq!(frame.tab_bar_height, 20);
    assert_eq!(
        *frame.root_window.bounds(),
        Rect::new(0.0, 20.0, 400.0, 224.0)
    );
    assert_eq!(
        *frame.minibuffer_leaf.as_ref().unwrap().bounds(),
        Rect::new(0.0, 244.0, 400.0, 16.0)
    );
    assert_eq!(frame.parameter("height"), Some(Value::fixnum(12)));
}

#[test]
fn grow_and_shrink_mini_window_adjusts_bounds() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 80, 24, BufferId(1));
    // Treat the frame as a TTY-style frame where 1 px == 1 character row.
    // char_height=1.0 means `grow_mini_window` grows by 1 row per delta,
    // and max-mini-window-height (25% of 24 rows = 6 rows) is comfortably
    // above the 1-row minimum.
    // Re-initialize the minibuffer to exactly 1 row so that it starts at
    // the minimum height and has room to grow.
    {
        let frame = mgr.get_mut(fid).unwrap();
        frame.char_height = 1.0;
        frame.char_width = 1.0;
        if let Some(mini) = frame.minibuffer_leaf.as_mut() {
            let mut b = *mini.bounds();
            b.height = 1.0;
            mini.set_bounds(b);
        }
        frame.sync_window_area_bounds();
    }
    let frame = mgr.get(fid).unwrap();
    let initial_mini_h = frame.minibuffer_leaf.as_ref().unwrap().bounds().height;

    mgr.get_mut(fid).unwrap().grow_mini_window(3);
    let grown_h = mgr
        .get(fid)
        .unwrap()
        .minibuffer_leaf
        .as_ref()
        .unwrap()
        .bounds()
        .height;
    assert!(
        grown_h > initial_mini_h,
        "minibuffer should grow: initial={initial_mini_h} grown={grown_h}"
    );

    mgr.get_mut(fid).unwrap().shrink_mini_window();
    let shrunk_h = mgr
        .get(fid)
        .unwrap()
        .minibuffer_leaf
        .as_ref()
        .unwrap()
        .bounds()
        .height;
    assert!(
        shrunk_h < grown_h,
        "minibuffer should shrink: grown={grown_h} shrunk={shrunk_h}"
    );
}

#[test]
fn grow_mini_window_with_explicit_max_lines_honors_integer_limit() {
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 80, 24, BufferId(1));
    {
        let frame = mgr.get_mut(fid).unwrap();
        frame.char_height = 1.0;
        frame.char_width = 1.0;
        if let Some(mini) = frame.minibuffer_leaf.as_mut() {
            let mut b = *mini.bounds();
            b.height = 1.0;
            mini.set_bounds(b);
        }
        frame.sync_window_area_bounds();
    }

    mgr.get_mut(fid)
        .unwrap()
        .grow_mini_window_with_max_lines(20, 8.0);
    let grown_h = mgr
        .get(fid)
        .unwrap()
        .minibuffer_leaf
        .as_ref()
        .unwrap()
        .bounds()
        .height;

    assert_eq!(
        grown_h, 8.0,
        "explicit integer max-mini-window-height should win"
    );
}

#[test]
fn grow_mini_window_with_max_lines_one_caps_at_one_row_not_whole_frame() {
    // Regression: `max-mini-window-height = 1` (an integer 1 line, e.g. set
    // buffer-local by vertico-posframe) must cap the mini-window at ONE row.
    // A prior bug treated `max_lines <= 1.0` as a *fraction* of the frame, so a
    // 1-line cap grew the mini-window to the whole frame and crushed the main
    // window (the `SPC SPC` posframe "echo area fills the main frame" bug).
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 80, 24, BufferId(1));
    {
        let frame = mgr.get_mut(fid).unwrap();
        frame.char_height = 1.0;
        frame.char_width = 1.0;
        if let Some(mini) = frame.minibuffer_leaf.as_mut() {
            let mut b = *mini.bounds();
            b.height = 1.0;
            mini.set_bounds(b);
        }
        frame.sync_window_area_bounds();
    }

    // Ask to grow by 34 rows (as vertico's 35-candidate overlay would), capped
    // at a 1-line `max-mini-window-height`.
    mgr.get_mut(fid)
        .unwrap()
        .grow_mini_window_with_max_lines(34, 1.0);
    let grown_h = mgr
        .get(fid)
        .unwrap()
        .minibuffer_leaf
        .as_ref()
        .unwrap()
        .bounds()
        .height;

    assert_eq!(
        grown_h, 1.0,
        "a 1-line cap must keep the mini-window at one row, not grow to the frame"
    );
}

#[test]
fn grow_mini_window_lands_on_whole_rows_of_the_current_unit() {
    // GNU `grow_mini_window` adds the pixel difference between content and
    // box text height (window.c:5896-5930), so the mini-window ends at the
    // content height: for row content, whole rows of the current line height.
    // A mini-window left at 11px under a 17px font (font change, restored
    // configuration) growing by one row must land on 17px, not 28px.
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 502, 430, BufferId(1));
    {
        let frame = mgr.get_mut(fid).unwrap();
        frame.char_height = 17.0;
        frame.char_width = 6.0;
        let mini = frame.minibuffer_leaf.as_mut().unwrap();
        let mut b = *mini.bounds();
        b.height = 11.0;
        mini.set_bounds(b);
        frame.sync_window_area_bounds();
    }

    mgr.get_mut(fid)
        .unwrap()
        .grow_mini_window_with_max_lines(1, 8.0);
    let frame = mgr.get(fid).unwrap();
    let mini = frame.minibuffer_leaf.as_ref().unwrap().bounds();
    assert_eq!(mini.height, 17.0, "one row of the current 17px unit");
    assert_eq!(
        mini.y + mini.height,
        430.0,
        "mini-window ends at the frame bottom"
    );
    assert_eq!(
        frame.root_window.bounds().y + frame.root_window.bounds().height,
        mini.y,
        "root window ends where the mini-window starts"
    );

    // A stale 22px base (two 11px rows) under a 19px font growing by two rows
    // lands on three rows of 19px, as GNU's pixel delta would.
    {
        let frame = mgr.get_mut(fid).unwrap();
        frame.char_height = 19.0;
        let mini = frame.minibuffer_leaf.as_mut().unwrap();
        let mut b = *mini.bounds();
        b.height = 22.0;
        mini.set_bounds(b);
        frame.sync_window_area_bounds();
        frame.grow_mini_window_with_max_lines(2, 8.0);
    }
    assert_eq!(
        mgr.get(fid)
            .unwrap()
            .minibuffer_leaf
            .as_ref()
            .unwrap()
            .bounds()
            .height,
        57.0
    );
}

#[test]
fn mini_window_rows_tolerates_float_division_at_whole_rows() {
    // 3 * 11.9 divides to 2.9999998 in f32; that is three rows, not two.
    assert_eq!(mini_window_rows(3.0 * 11.9, 11.9), 3);
    assert_eq!(mini_window_rows(11.0, 17.0), 0);
    assert_eq!(mini_window_rows(22.0, 19.0), 1);
    assert_eq!(mini_window_rows(0.0, 17.0), 0);
}

#[test]
fn grow_mini_window_always_moves_from_a_whole_row_count_at_a_fractional_unit() {
    // A three-row mini-window at an 11.9px unit asked to grow by one row must
    // land on four rows, not on its own height (which would make the engine
    // retry until its layout budget was gone).
    crate::test_utils::init_test_tracing();
    let mut mgr = FrameManager::new();
    let fid = mgr.create_frame("F1", 502, 430, BufferId(1));
    {
        let frame = mgr.get_mut(fid).unwrap();
        frame.char_height = 11.9;
        frame.char_width = 6.0;
        let mini = frame.minibuffer_leaf.as_mut().unwrap();
        let mut b = *mini.bounds();
        b.height = 3.0 * 11.9;
        mini.set_bounds(b);
        frame.sync_window_area_bounds();
        frame.grow_mini_window_with_max_lines(1, 8.0);
    }
    let h = mgr
        .get(fid)
        .unwrap()
        .minibuffer_leaf
        .as_ref()
        .unwrap()
        .bounds()
        .height;
    assert!((h - 4.0 * 11.9).abs() < 0.01, "expected four rows, got {h}");
}
