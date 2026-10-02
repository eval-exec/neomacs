use super::*;
use crate::window::{
    DisplayPointRole, DisplayPointRow, DisplayPointRows, DisplayPointSnapshot, DisplayRowSnapshot,
    PresentedBodyRowSnapshot, WindowCursorKind, WindowCursorPos, WindowCursorSnapshot,
};
use std::sync::Arc;

fn point(pos: i64, row: i64, col: i64, x: i64) -> DisplayPointSnapshot {
    DisplayPointSnapshot {
        buffer_pos: LispCharPos1::new(pos),
        role: DisplayPointRole::Glyph,
        x,
        y: 900, // Published positions use the numeric body mapping, not point y.
        width: 8,
        height: 12,
        row,
        col,
    }
}

fn mapping(output_row: i64, body_row: i64, body_y: i64) -> PresentedBodyRowSnapshot {
    PresentedBodyRowSnapshot {
        output_row,
        body_row,
        body_y,
    }
}

fn make_snapshot(
    rows: Vec<DisplayPointRow>,
    body_rows: Vec<PresentedBodyRowSnapshot>,
) -> WindowDisplaySnapshot {
    WindowDisplaySnapshot {
        window_id: WindowId(11),
        point_rows: Some(DisplayPointRows { rows }),
        body_rows,
        regions_materialized: true,
        regions: PresentedWindowRegions {
            outer: TransportRect::new(50.0, 30.0, 300.0, 200.0),
            text_body: TransportRect::new(58.0, 40.0, 280.0, 180.0),
            ..PresentedWindowRegions::default()
        },
        ..WindowDisplaySnapshot::default()
    }
}

fn pair(snapshot: &WindowDisplaySnapshot) -> (PresentationWindow, PresentationWindow) {
    (
        PresentationWindow::from_snapshot_with_mode(snapshot, GeometryPositionsMode::Off).unwrap(),
        PresentationWindow::from_snapshot_with_mode(snapshot, GeometryPositionsMode::On).unwrap(),
    )
}

fn view(window: &PresentationWindow) -> SnapshotWindowGeometry<'_> {
    SnapshotWindowGeometry::new(PresentationId::new(1), FrameId(1), window.id, window).unwrap()
}

#[test]
fn deferred_positions_match_hidden_bidi_coordinate_and_mixed_height_queries() {
    let mut marker = point(4, 0, 1, 8);
    marker.role = DisplayPointRole::OverlaidMarker;
    marker.width = 0;
    let mut snapshot = make_snapshot(
        vec![
            DisplayPointRow::from_points(vec![point(1, 0, 0, 24), marker, point(5, 0, 2, 0)]),
            DisplayPointRow::from_points(vec![point(8, 1, 0, 8), point(10, 1, 1, 0)]),
        ],
        vec![mapping(0, 2, 16), mapping(1, 3, 48)],
    );
    // Preserve last-wins height semantics even with duplicate output metrics.
    snapshot.rows = vec![
        DisplayRowSnapshot {
            row: 0,
            height: 2,
            ..DisplayRowSnapshot::default()
        },
        DisplayRowSnapshot {
            row: 0,
            height: 32,
            ..DisplayRowSnapshot::default()
        },
    ];
    let (eager, deferred) = pair(&snapshot);
    assert!(!deferred.positions.is_materialized());
    assert_eq!(
        view(&deferred)
            .point_for_buffer_pos(LispCharPos1::new(2))
            .unwrap()
            .unwrap()
            .buffer_pos(),
        LispCharPos1::new(4)
    );
    for pos in 1..=12 {
        assert_eq!(
            view(&eager).point_for_buffer_pos(LispCharPos1::new(pos)),
            view(&deferred).point_for_buffer_pos(LispCharPos1::new(pos))
        );
    }
    for y in [-1, 9, 25, 30, 45, 57, 58, 65, 80] {
        for x in [-5, 0, 4, 8, 16, 24, 100] {
            assert_eq!(
                view(&eager).point_at_window_coords(x, y),
                view(&deferred).point_at_window_coords(x, y),
                "x={x},y={y}"
            );
        }
    }
    assert_eq!(eager.positions.as_slice(), deferred.positions.as_slice());
    assert_eq!(eager.positions.as_slice()[0].row_height, 32);
    assert_eq!(eager.positions.as_slice()[3].row_height, 12);
    assert_eq!(eager, deferred);
}

#[test]
fn physical_cursor_dimensions_do_not_materialize_deferred_positions() {
    let mut snapshot = make_snapshot(
        vec![DisplayPointRow::from_points(vec![point(1, 4, 0, 0)])],
        vec![mapping(4, 7, 40)],
    );
    snapshot.logical_cursor = Some(WindowCursorPos {
        row: 90,
        col: 91,
        x: 20,
        y: 30,
    });
    snapshot.phys_cursor = Some(WindowCursorSnapshot {
        kind: WindowCursorKind::Bar,
        row: 7,
        col: 0,
        x: 0,
        y: 40,
        width: 3,
        height: 19,
        ascent: 14,
    });
    let (eager, deferred) = pair(&snapshot);
    assert!(!deferred.positions.is_materialized());
    assert_eq!(eager.cursor, deferred.cursor);
    assert_eq!(
        deferred.cursor,
        Some(PresentationCursor {
            x: 20,
            body_y: 30,
            width: 3,
            height: 19
        })
    );
    assert!(!deferred.positions.is_materialized());
    // With no explicit logical cursor the physical position itself is used.
    snapshot.logical_cursor = None;
    let (eager, deferred) = pair(&snapshot);
    assert_eq!(eager.cursor, deferred.cursor);
    assert_eq!(deferred.cursor.unwrap().body_y, 40);
    assert!(!deferred.positions.is_materialized());
}

#[test]
fn logical_cursor_fallback_preserves_marker_and_descriptor_tie_order() {
    let mut marker = point(4, 9, 3, 0);
    marker.role = DisplayPointRole::OverlaidMarker;
    marker.width = 11;
    marker.height = 13;
    let mut glyph = point(4, 2, 3, 0);
    glyph.width = 22;
    let mut snapshot = make_snapshot(
        vec![
            DisplayPointRow::from_points(vec![marker.clone()]),
            DisplayPointRow::from_points(vec![glyph]),
        ],
        vec![mapping(9, 7, 20), mapping(2, 7, 20), mapping(4, 7, 20)],
    );
    snapshot.logical_cursor = Some(WindowCursorPos {
        row: 7,
        col: 3,
        x: 99,
        y: 20,
    });
    let (eager, deferred) = pair(&snapshot);
    assert_eq!(eager.cursor, deferred.cursor);
    assert_eq!(
        deferred.cursor.unwrap().width,
        11,
        "Vec index wins equal-position ties, including markers"
    );
    assert!(!deferred.positions.is_materialized());
    let mut earlier = point(3, 4, 3, 0);
    earlier.width = 33;
    snapshot
        .point_rows
        .as_mut()
        .unwrap()
        .rows
        .push(DisplayPointRow::from_points(vec![earlier]));
    let (eager, deferred) = pair(&snapshot);
    assert_eq!(eager.cursor, deferred.cursor);
    assert_eq!(
        deferred.cursor.unwrap().width,
        33,
        "source position wins before descriptor index"
    );
    assert!(!deferred.positions.is_materialized());
    // Inside one row, source-order col/x ties retain original stable order.
    let mut second_marker = marker.clone();
    second_marker.width = 44;
    snapshot.point_rows = Some(DisplayPointRows {
        rows: vec![DisplayPointRow::from_points(vec![marker, second_marker])],
    });
    let (eager, deferred) = pair(&snapshot);
    assert_eq!(eager.cursor, deferred.cursor);
    assert_eq!(deferred.cursor.unwrap().width, 11);
    snapshot.logical_cursor.as_mut().unwrap().col = 100;
    let (eager, deferred) = pair(&snapshot);
    assert_eq!(eager.cursor, None);
    assert_eq!(eager.cursor, deferred.cursor);
    assert!(!deferred.positions.is_materialized());
}

#[test]
fn empty_wide_and_shifted_rows_preserve_positions_without_flattening_at_publish() {
    let mut wide = point(1_i64 << 40, 8, 70_000, i64::MAX - 100);
    wide.width = 90_000;
    wide.height = 70_000;
    wide.y = -4;
    let row = DisplayPointRow::from_points(vec![wide]);
    assert!(!row.is_compact());
    let shifted = row.replaced_placement(2, 44, 5);
    assert!(row.shares_cells_with(&shifted));
    let snapshot = make_snapshot(
        vec![
            DisplayPointRow::from_placement(90, 0, 0, Vec::new()),
            shifted,
        ],
        vec![mapping(2, 3, 48)],
    );
    let (eager, deferred) = pair(&snapshot);
    assert!(!deferred.positions.is_materialized());
    assert_eq!(eager.positions.as_slice(), deferred.positions.as_slice());
    let point = deferred.positions.as_slice()[0];
    assert_eq!(point.buffer_pos, LispCharPos1::new((1_i64 << 40) + 5));
    assert_eq!(
        (point.body_y, point.width, point.height, point.row_height),
        (48, 90_000, 70_000, 70_000)
    );
    let empty = make_snapshot(Vec::new(), Vec::new());
    let (eager, deferred) = pair(&empty);
    assert!(!deferred.positions.is_materialized());
    assert_eq!(eager, deferred);
    assert!(deferred.positions.as_slice().is_empty());
}

#[test]
fn validation_errors_keep_canonical_missing_row_and_upfront_duplicate_order() {
    let mut snapshot = make_snapshot(
        vec![
            DisplayPointRow::from_placement(90, 0, 0, Vec::new()),
            DisplayPointRow::from_points(vec![point(4, 9, 0, 0)]),
            DisplayPointRow::from_points(vec![point(3, 7, 0, 0)]),
            DisplayPointRow::from_points(vec![point(3, 2, 0, 0)]),
        ],
        Vec::new(),
    );
    let check = |snapshot: &WindowDisplaySnapshot, expected| {
        for mode in [GeometryPositionsMode::Off, GeometryPositionsMode::On] {
            assert_eq!(
                PresentationWindow::from_snapshot_with_mode(snapshot, mode),
                Err(expected)
            );
        }
    };
    check(
        &snapshot,
        GeometryError::MissingBodyRow {
            window: snapshot.window_id,
            output_row: 7,
        },
    );
    snapshot.body_rows = vec![mapping(100, 0, 0), mapping(100, 1, 1)];
    check(
        &snapshot,
        GeometryError::DuplicateBodyRow {
            window: snapshot.window_id,
            output_row: 100,
        },
    );
    snapshot.regions.outer.width = -1.0;
    check(&snapshot, GeometryError::InvalidExtent);
}

#[test]
fn legacy_flat_on_arm_keeps_eager_traversal_and_error_order() {
    assert_eq!(
        GeometryPositionsMode::from_setting(None),
        GeometryPositionsMode::On
    );
    for setting in [Some(""), Some("off"), Some("unknown")] {
        assert_eq!(
            GeometryPositionsMode::from_setting(setting),
            GeometryPositionsMode::Off
        );
    }
    for setting in ["on", "1", "true", "yes", " ON ", "True", "YES"] {
        assert_eq!(
            GeometryPositionsMode::from_setting(Some(setting)),
            GeometryPositionsMode::On
        );
    }
    let mut snapshot = make_snapshot(Vec::new(), Vec::new());
    snapshot.point_rows = None;
    snapshot.points = vec![point(99, 9, 0, 0), point(1, 2, 0, 0)];
    for mode in [GeometryPositionsMode::Off, GeometryPositionsMode::On] {
        assert_eq!(
            PresentationWindow::from_snapshot_with_mode(&snapshot, mode),
            Err(GeometryError::MissingBodyRow {
                window: snapshot.window_id,
                output_row: 9
            })
        );
    }
    snapshot.body_rows = vec![mapping(9, 7, 20), mapping(2, 3, 40)];
    let (eager, on) = pair(&snapshot);
    assert!(on.positions.is_materialized());
    assert_eq!(eager, on);
    assert_eq!(on.positions.as_slice()[0].buffer_pos, LispCharPos1::new(99));
    assert_eq!(on.positions.as_slice()[1].buffer_pos, LispCharPos1::ONE);
}

#[test]
fn deferred_publications_own_numeric_metadata_and_share_one_concurrent_cache() {
    let mut snapshot = make_snapshot(
        vec![DisplayPointRow::from_points(vec![
            point(1, 2, 0, 0),
            point(4, 2, 1, 8),
        ])],
        vec![mapping(2, 3, 40)],
    );
    let (eager, deferred) = pair(&snapshot);
    let clone = deferred.clone();
    assert!(!deferred.positions.is_materialized());
    assert!(!clone.positions.is_materialized());
    snapshot.body_rows[0].body_y = 500;
    snapshot.point_rows = None;
    snapshot.points = vec![point(100, 100, 100, 100)];
    let publication = Arc::new(deferred);
    let expected = eager.positions.as_slice().to_vec();
    let readers: Vec<_> = (0..8)
        .map(|_| {
            let publication = Arc::clone(&publication);
            let expected = expected.clone();
            std::thread::spawn(move || {
                let positions = publication.positions.as_slice();
                assert_eq!(positions, expected);
                positions.as_ptr() as usize
            })
        })
        .collect();
    let pointers: Vec<_> = readers
        .into_iter()
        .map(|reader| reader.join().unwrap())
        .collect();
    assert!(pointers.iter().all(|pointer| *pointer == pointers[0]));
    assert!(clone.positions.is_materialized());
    assert_eq!(clone.positions.as_slice().as_ptr() as usize, pointers[0]);
    assert_eq!(clone, eager);
}

#[test]
fn semantic_equality_and_debug_materialize_the_same_original_vector() {
    let snapshot = make_snapshot(
        vec![DisplayPointRow::from_points(vec![point(1, 2, 0, 0)])],
        vec![mapping(2, 3, 40)],
    );
    let (eager, deferred) = pair(&snapshot);
    assert!(!deferred.positions.is_materialized());
    assert_eq!(format!("{deferred:?}"), format!("{eager:?}"));
    assert!(deferred.positions.is_materialized());
    let (_, deferred) = pair(&snapshot);
    assert!(!deferred.positions.is_materialized());
    assert_eq!(eager, deferred);
    assert!(deferred.positions.is_materialized());
}
