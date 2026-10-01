use std::sync::Arc;

use neomacs_display_protocol::{
    DisplayWindowId, FrameRect, PresentationId, PresentedHitIndex, PresentedHitQuery,
    PresentedHitRegion, PresentedRegionKind, PresentedTextPosition, Rect,
};
use neovm_core::buffer::LispCharPos1;
use neovm_core::window::{
    DisplayPointRole, DisplayPointRows, DisplayPointSnapshot, DisplayRowSnapshot,
    PresentedBodyRowSnapshot, WindowDisplaySnapshot,
};

use super::RowWindowText;

fn point(pos: i64, row: i64, x: i64, width: i64, height: i64) -> DisplayPointSnapshot {
    DisplayPointSnapshot {
        buffer_pos: LispCharPos1::new(pos),
        role: DisplayPointRole::Glyph,
        row,
        col: pos,
        x,
        y: row * 10,
        width,
        height,
    }
}

fn fixture() -> (Arc<WindowDisplaySnapshot>, Rect) {
    // Buffer order differs from visual x order. Tall glyphs overlap other rows,
    // row 3 is empty, and a wide-encoded row retains unusual signed geometry.
    let mut points = vec![
        point(4, 0, 9, 8, 25),
        point(2, 0, -5, 12, 8),
        point(3, 0, 5, 9, 8),
        point(20, 1, 0, 18, 8),
        point(12, 1, 4, 5, 8),
        point(30, 2, 7, 0, 0),
        point(31, 2, 20, 12, 8),
        point(40, 4, i64::MAX, i64::MAX, i64::MAX),
        point(41, 4, -10, 16, 4),
    ];
    let mut marker = point(3, 0, 6, 4, 8);
    marker.role = DisplayPointRole::OverlaidMarker;
    marker.col = 100;
    points.push(marker);
    points.sort_by_key(|point| (point.buffer_pos, point.row, point.col, point.x));
    let rows: Vec<_> = (0..5)
        .map(|row| DisplayRowSnapshot {
            row,
            y: row * 10,
            height: 10,
            start_buffer_pos: Some(LispCharPos1::new(row * 10 + 1)),
            end_buffer_pos: Some(LispCharPos1::new(row * 10 + 9)),
            ..DisplayRowSnapshot::default()
        })
        .collect();
    let body_rows = rows
        .iter()
        .map(|row| PresentedBodyRowSnapshot {
            output_row: row.row,
            body_row: row.row,
            body_y: row.y,
        })
        .collect();
    let snapshot = WindowDisplaySnapshot {
        points: points.clone(),
        point_rows: Some(DisplayPointRows::from_points(points)),
        rows,
        body_rows,
        ..WindowDisplaySnapshot::default()
    };
    (Arc::new(snapshot), Rect::new(12.0, 7.0, 80.0, 55.0))
}

fn contains(position: PresentedTextPosition, x: f32, y: f32) -> bool {
    let bounds = position.bounds();
    x >= bounds.x()
        && x < bounds.x() + bounds.width()
        && y >= bounds.y()
        && y < bounds.y() + bounds.height()
}

fn materialize(snapshot: &WindowDisplaySnapshot, body: Rect) -> Vec<PresentedTextPosition> {
    // Keep the legacy source vector from fixture construction as an independent
    // oracle for both source order and position serialization.
    let mut legacy = snapshot.clone();
    legacy.point_rows = None;
    super::super::body_text_positions(DisplayWindowId::new(1), &legacy, body).unwrap()
}

#[test]
fn indexed_row_hits_match_flattened_precedence_at_every_pixel() {
    let (snapshot, body) = fixture();
    assert_eq!(snapshot.iter_points().collect::<Vec<_>>(), snapshot.points);
    let positions = materialize(&snapshot, body);
    let source = RowWindowText::new(DisplayWindowId::new(1), snapshot, body)
        .unwrap()
        .unwrap();
    for y in 0..75 {
        for x in 0..110 {
            let (x, y) = (x as f32 + 0.5, y as f32 + 0.5);
            let expected = positions
                .iter()
                .copied()
                .find(|position| contains(*position, x, y));
            assert_eq!(source.hit(x, y).unwrap(), expected, "({x}, {y})");
        }
    }
}

#[test]
fn querying_one_row_leaves_other_row_caches_unbuilt() {
    let (snapshot, body) = fixture();
    let source = RowWindowText::new(DisplayWindowId::new(1), snapshot, body)
        .unwrap()
        .unwrap();
    assert!(source.rows.iter().all(|row| row.built.get().is_none()));
    assert!(source.hit(13.0, 8.0).unwrap().is_some());
    let built = source
        .rows
        .iter()
        .filter(|row| row.built.get().is_some())
        .count();
    assert_eq!(
        built, 2,
        "only the first glyph and fallback row are queried"
    );
    assert!(built < source.rows.len());
}

fn indices(
    snapshot: Arc<WindowDisplaySnapshot>,
    body: Rect,
) -> (PresentedHitIndex, PresentedHitIndex) {
    let window = DisplayWindowId::new(1);
    let region = PresentedHitRegion::new(
        Some(window),
        PresentedRegionKind::TextBody,
        FrameRect::new(body.x, body.y, body.width, body.height).unwrap(),
        0,
    );
    let eager = PresentedHitIndex::from_parts(
        PresentationId::new(7),
        vec![region],
        materialize(&snapshot, body),
    )
    .unwrap();
    let row_text = RowWindowText::new(window, snapshot.clone(), body).unwrap();
    let source = super::super::DeferredFrameText {
        windows: vec![super::super::DeferredWindowText {
            window,
            snapshot,
            text_body: body,
            row_text,
        }],
    };
    let deferred = PresentedHitIndex::from_parts(PresentationId::new(7), vec![region], Vec::new())
        .unwrap()
        .with_deferred_text(Arc::new(source));
    (eager, deferred)
}

fn query(index: &PresentedHitIndex, x: f32, y: f32) -> Option<PresentedTextPosition> {
    let point = neomacs_display_protocol::InteractionProjection::settled(index.presentation())
        .map(
            neomacs_display_protocol::GeometryPoint::<
                neomacs_display_protocol::RootSurfaceSpace,
                neomacs_display_protocol::LogicalPixels,
            >::from_px(x, y)
            .unwrap(),
        )
        .unwrap();
    index
        .resolve(PresentedHitQuery::new(point))
        .unwrap()
        .and_then(|hit| hit.text_position())
}

#[test]
fn direct_window_source_keeps_text_deferred_until_serialization() {
    let (snapshot, body) = fixture();
    let (eager, deferred) = indices(snapshot, body);
    assert_eq!(query(&deferred, 16.0, 22.0), query(&eager, 16.0, 22.0));
    assert_eq!(query(&deferred, 16.0, 8.0), query(&eager, 16.0, 8.0));
    assert!(deferred.text_deferred());
    assert_eq!(
        serde_json::to_string(&deferred).unwrap(),
        serde_json::to_string(&eager).unwrap()
    );
    assert!(!deferred.text_deferred());
    assert_eq!(deferred, eager);
}

#[test]
fn flat_snapshot_falls_back_and_missing_body_row_is_rejected_globally() {
    let (snapshot, body) = fixture();
    let mut flat = (*snapshot).clone();
    flat.points = flat.iter_points().collect();
    flat.point_rows = None;
    assert!(
        RowWindowText::new(DisplayWindowId::new(1), Arc::new(flat), body)
            .unwrap()
            .is_none()
    );
    let mut missing = (*snapshot).clone();
    missing.body_rows.retain(|row| row.output_row != 4);
    assert!(matches!(
        RowWindowText::new(DisplayWindowId::new(1), Arc::new(missing), body),
        Err(neomacs_display_protocol::PresentedHitError::MissingBodyRow { output_row: 4, .. })
    ));
}

#[test]
fn empty_row_fallbacks_and_clipped_body_are_exact() {
    let (snapshot, body) = fixture();
    let source = RowWindowText::new(DisplayWindowId::new(1), snapshot.clone(), body)
        .unwrap()
        .unwrap();
    assert_eq!(
        source.hit(60.0, 42.0).unwrap().unwrap().buffer_position(),
        31
    );
    assert_eq!(source.hit(60.0, 42.0).unwrap().unwrap().row(), 3);
    let clipped = RowWindowText::new(
        DisplayWindowId::new(1),
        snapshot,
        Rect::new(0.0, 0.0, 0.0, 0.0),
    )
    .unwrap()
    .unwrap();
    assert_eq!(clipped.is_empty(), Some(true));
    assert!(clipped.hit(0.0, 0.0).unwrap().is_none());
}

#[test]
fn row_sources_allow_concurrent_queries_without_frame_materialization() {
    let (snapshot, body) = fixture();
    let (eager, deferred) = indices(snapshot, body);
    let expected = query(&eager, 16.0, 22.0);
    let deferred = Arc::new(deferred);
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let index = deferred.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    assert_eq!(query(&index, 16.0, 22.0), expected);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert!(deferred.text_deferred());
}
