//! Row-lazy hits retain the canonical source roles used by eager publication.
//! Isolate with NEOMACS_POSN_OBJECT_EXTENT=on before process startup.

use super::RowWindowText;
use neomacs_display_protocol::{DisplayWindowId, Rect};
use neovm_core::buffer::LispCharPos1;
use neovm_core::window::{
    DisplayPointRole, DisplayPointRows, DisplayPointSnapshot, DisplayRowSnapshot,
    PresentedBodyRowSnapshot, WindowDisplaySnapshot,
};
use std::sync::Arc;

#[test]
fn indexed_row_hits_preserve_eager_canonical_point_roles() {
    assert!(
        neovm_core::window::posn_object_extent_mode().enabled(),
        "isolate with NEOMACS_POSN_OBJECT_EXTENT=on before child startup"
    );
    let roles = [
        DisplayPointRole::Glyph,
        DisplayPointRole::InsertionBoundary,
        DisplayPointRole::OverlaidMarker,
        DisplayPointRole::SyntheticBoundary,
    ];
    let points: Vec<_> = roles
        .iter()
        .enumerate()
        .map(|(column, role)| DisplayPointSnapshot {
            buffer_pos: LispCharPos1::new(column as i64 + 1),
            role: *role,
            x: column as i64,
            y: 0,
            width: 1,
            height: 1,
            row: 0,
            col: column as i64,
        })
        .collect();
    let snapshot = WindowDisplaySnapshot {
        points: points.clone(),
        point_rows: Some(DisplayPointRows::from_points(points)),
        rows: vec![DisplayRowSnapshot {
            row: 0,
            y: 0,
            height: 1,
            start_buffer_pos: Some(LispCharPos1::ONE),
            end_buffer_pos: Some(LispCharPos1::new(4)),
            ..Default::default()
        }],
        body_rows: vec![PresentedBodyRowSnapshot {
            output_row: 0,
            body_row: 0,
            body_y: 0,
        }],
        ..Default::default()
    };
    let body = Rect::new(0.0, 0.0, 4.0, 1.0);
    let mut flat = snapshot.clone();
    flat.point_rows = None;
    let eager = super::super::body_text_positions(DisplayWindowId::new(1), &flat, body)
        .expect("eager canonical source");
    let lazy = RowWindowText::new(DisplayWindowId::new(1), Arc::new(snapshot), body)
        .expect("row source")
        .expect("shared row source");
    for (column, role) in roles.into_iter().enumerate() {
        let expected = eager
            .iter()
            .find(|position| position.buffer_position() == column as i64 + 1)
            .copied()
            .expect("eager source position");
        assert_eq!(expected.point_role(), role, "eager preserves producer role");
        assert_eq!(
            lazy.hit(column as f32 + 0.5, 0.5).expect("row query"),
            Some(expected),
            "row-lazy publication must retain every canonical role at column {column}"
        );
    }
}
