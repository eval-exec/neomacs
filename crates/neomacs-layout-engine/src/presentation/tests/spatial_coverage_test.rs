//! Coverage shares visible presentation's immutable row source and exact fallback.

use std::sync::Arc;

use neomacs_display_protocol::PresentedTextRegionValidation::{Unsupported, Validated};
use neomacs_display_protocol::{
    DisplayWindowId, FrameRect, PresentationId, PresentedHitError, PresentedHitIndex,
    PresentedHitQuery, PresentedTextPosition, PresentedTextPositionSource, Rect,
};
use neovm_core::buffer::LispCharPos1;
use neovm_core::window::{
    DisplayPointRole, DisplayPointRows, DisplayPointSnapshot, DisplayRowSnapshot,
    PosnObjectExtentMode, PresentedBodyRowSnapshot, WindowDisplaySnapshot,
};

fn window() -> DisplayWindowId {
    DisplayWindowId::new(1)
}

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

fn body_rect(body: Rect) -> FrameRect {
    FrameRect::new(body.x, body.y, body.width, body.height).unwrap()
}

fn source(snapshot: Arc<WindowDisplaySnapshot>, body: Rect) -> super::DeferredFrameText {
    let row_text = super::row_text::RowWindowText::new(window(), snapshot.clone(), body)
        .unwrap()
        .unwrap();
    super::DeferredFrameText {
        windows: vec![super::DeferredWindowText {
            window: window(),
            snapshot,
            text_body: body,
            row_text: Some(row_text),
        }],
    }
}

fn index(snapshot: Arc<WindowDisplaySnapshot>, body: Rect) -> PresentedHitIndex {
    super::coverage_hit_index(PresentationId::new(7), window(), snapshot, body).unwrap()
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

fn expected_hit(
    positions: &[PresentedTextPosition],
    x: f32,
    y: f32,
) -> Option<PresentedTextPosition> {
    positions.iter().copied().find(|position| {
        let bounds = position.bounds();
        x >= bounds.x()
            && x < bounds.x() + bounds.width()
            && y >= bounds.y()
            && y < bounds.y() + bounds.height()
    })
}

fn single_row(
    points: Vec<DisplayPointSnapshot>,
    fallback_rows: usize,
) -> Arc<WindowDisplaySnapshot> {
    let last = points
        .iter()
        .map(|point| point.buffer_pos)
        .max()
        .unwrap_or(LispCharPos1::ONE);
    Arc::new(WindowDisplaySnapshot {
        point_rows: Some(DisplayPointRows::from_points(points)),
        rows: vec![
            DisplayRowSnapshot {
                row: 0,
                y: 0,
                height: 1,
                start_buffer_pos: Some(LispCharPos1::ONE),
                end_buffer_pos: Some(last),
                ..Default::default()
            };
            fallback_rows
        ],
        body_rows: vec![PresentedBodyRowSnapshot {
            output_row: 0,
            body_row: 0,
            body_y: 0,
        }],
        ..Default::default()
    })
}

fn complex_fixture() -> (Arc<WindowDisplaySnapshot>, Rect) {
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
    let rows = (0..5)
        .map(|row| DisplayRowSnapshot {
            row,
            y: row * 10,
            height: 10,
            start_buffer_pos: Some(LispCharPos1::new(row * 10 + 1)),
            end_buffer_pos: Some(LispCharPos1::new(row * 10 + 9)),
            ..Default::default()
        })
        .collect();
    let body_rows = (0..5)
        .map(|row| PresentedBodyRowSnapshot {
            output_row: row,
            body_row: row,
            body_y: row * 10,
        })
        .collect();
    let mut snapshot = WindowDisplaySnapshot {
        point_rows: Some(DisplayPointRows::from_points(points)),
        rows,
        body_rows,
        ..Default::default()
    };
    snapshot.set_posn_object_extent_mode_for_test(Some(PosnObjectExtentMode::On));
    (Arc::new(snapshot), Rect::new(12.0, 7.0, 80.0, 55.0))
}

#[test]
fn coverage_row_source_matches_legacy_hits_without_flattening() {
    let (snapshot, body) = complex_fixture();
    let positions = super::body_text_positions(window(), &snapshot, body).unwrap();
    let source = source(snapshot.clone(), body);
    assert_eq!(
        source.validate_region(window(), body_rect(body), 65_536),
        Validated
    );
    assert_eq!(
        source.validate_region(DisplayWindowId::new(2), body_rect(body), 65_536),
        Unsupported
    );
    assert_eq!(
        source.validate_region(
            window(),
            FrameRect::new(body.x, body.y, body.width - 1.0, body.height).unwrap(),
            65_536,
        ),
        Unsupported
    );
    let index = index(snapshot, body);
    assert!(index.text_deferred());
    // Source order differs from visual order; tall glyphs overlap later rows,
    // row3 is empty, and row4 has signed wide cells. The legacy first hit wins.
    for (x, y) in [
        (13.0, 8.0),
        (22.0, 19.0),
        (28.0, 22.0),
        (60.0, 42.0),
        (14.0, 48.0),
        (91.5, 61.5),
        (92.0, 62.0),
    ] {
        assert_eq!(query(&index, x, y), expected_hit(&positions, x, y));
    }
    assert!(
        index.text_deferred(),
        "row queries must leave the global vector cold"
    );
    assert_eq!(index.text_positions(), positions.as_slice());
}

#[test]
fn coverage_source_rejects_malformed_unqueried_rows() {
    let (snapshot, body) = complex_fixture();
    let mut malformed = (*snapshot).clone();
    // The far-right point on row4 would never win the first-row query, but
    // its missing canonical body owner must still refuse publication.
    malformed.body_rows.retain(|row| row.output_row != 4);
    assert!(matches!(
        super::coverage_hit_index(PresentationId::new(7), window(), Arc::new(malformed), body),
        Err(PresentedHitError::MissingBodyRow { output_row: 4, .. })
    ));
}

#[test]
fn duplicate_or_unordered_point_rows_retain_eager_legacy_order() {
    let (snapshot, body) = complex_fixture();
    for duplicate in [false, true] {
        let mut ambiguous = (*snapshot).clone();
        let point_rows = ambiguous.point_rows.as_mut().unwrap();
        if duplicate {
            point_rows.rows.insert(1, point_rows.rows[0].clone());
        } else {
            point_rows.rows.swap(0, 1);
        }
        let expected = super::body_text_positions(window(), &ambiguous, body).unwrap();
        assert!(
            super::row_text::RowWindowText::new(window(), Arc::new(ambiguous.clone()), body)
                .unwrap()
                .is_none(),
            "ambiguous descriptors cannot become a direct row source"
        );
        let index = index(Arc::new(ambiguous), body);
        assert!(
            !index.text_deferred(),
            "ambiguous descriptors require eager validation"
        );
        assert_eq!(index.text_positions(), expected.as_slice());
        assert_eq!(
            query(&index, 22.0, 19.0),
            expected_hit(&expected, 22.0, 19.0)
        );
    }
}

#[test]
fn coverage_budget_counts_duplicate_fallback_row_multiplicity() {
    let snapshot = single_row(
        (0..64).map(|i| point(i + 1, 0, i * 2 + 1, 1, 1)).collect(),
        4,
    );
    let body = Rect::new(0.0, 0.0, 129.0, 1.0);
    let source = source(snapshot.clone(), body);
    let positions = super::body_text_positions(window(), &snapshot, body).unwrap();
    // 64 glyphs plus65 gap/tail spans for EACH of the4 fallback occurrences.
    assert_eq!(positions.len(), 324);
    assert_eq!(
        source.validate_region(window(), body_rect(body), 200),
        Unsupported
    );
    assert_eq!(
        source.validate_region(window(), body_rect(body), 324),
        Validated
    );
    let index = index(snapshot, body);
    assert_eq!(index.text_positions(), positions.as_slice());
}

#[test]
fn loose_over_budget_proof_retains_exact_clipped_positions() {
    let snapshot = single_row(
        (0..32_768)
            .map(|i| point(i + 1, 0, 1000 + i, 1, 1))
            .collect(),
        1,
    );
    let body = Rect::new(0.0, 0.0, 1.0, 1.0);
    let source = source(snapshot.clone(), body);
    // The safe upper bound is65,537, but all glyphs are outside the clip.
    // Exact admission sees only one fallback span and must remain possible.
    assert_eq!(
        source.validate_region(window(), body_rect(body), 65_536),
        Unsupported
    );
    let index = index(snapshot, body);
    let positions = index.text_positions();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].bounds(), body_rect(body));
    assert_eq!(query(&index, 0.5, 0.5), Some(positions[0]));
}

#[test]
fn fractional_and_wide_body_bounds_keep_exact_fallback_hits() {
    for body in [
        Rect::new(0.5, 0.25, 4.5, 1.5),
        Rect::new((1u32 << 21) as f32, 0.0, 4.0, 1.0),
    ] {
        let snapshot = single_row(vec![point(1, 0, 0, 1, 1)], 1);
        let expected = super::body_text_positions(window(), &snapshot, body).unwrap();
        let source = source(snapshot.clone(), body);
        assert_eq!(
            source.validate_region(window(), body_rect(body), 65_536),
            Unsupported
        );
        let index = index(snapshot, body);
        assert_eq!(index.text_positions(), expected.as_slice());
        let (x, y) = (body.x + 0.5, body.y + 0.5);
        assert_eq!(query(&index, x, y), expected_hit(&expected, x, y));
    }
}
