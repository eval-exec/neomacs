use super::*;
use crate::buffer::LispCharPos1;
use crate::window::{
    DisplayPointRole, DisplayPointRow, DisplayPointSnapshot, PointCell, PresentedBodyRowSnapshot,
    WindowId,
};
use neomacs_display_protocol::posn_object_extent::{PosnMatrixRow, PosnMatrixSnapshot};
use std::sync::Arc;

fn point(row: i64, col: i64, role: DisplayPointRole) -> DisplayPointSnapshot {
    DisplayPointSnapshot {
        buffer_pos: LispCharPos1::new(7),
        role,
        x: 3,
        y: 2,
        width: 17,
        height: 19,
        row,
        col,
    }
}

fn retained() -> WindowDisplaySnapshot {
    WindowDisplaySnapshot {
        window_id: WindowId(3),
        tab_line_height: 1,
        header_line_height: 1,
        body_rows: vec![PresentedBodyRowSnapshot {
            output_row: 2,
            body_row: 0,
            body_y: 0,
        }],
        posn_matrix: Some(Arc::new(PosnMatrixSnapshot {
            rows: vec![
                PosnMatrixRow {
                    enabled: true,
                    y: 0,
                    height: 1,
                    areas: [0, 4, 0],
                },
                PosnMatrixRow {
                    enabled: true,
                    y: 1,
                    height: 1,
                    areas: [0, 5, 0],
                },
                PosnMatrixRow {
                    enabled: true,
                    y: 2,
                    height: 1,
                    areas: [1, 3, 2],
                },
                PosnMatrixRow {
                    enabled: false,
                    y: 3,
                    height: 1,
                    areas: [9, 9, 9],
                },
            ],
        })),
        ..WindowDisplaySnapshot::default()
    }
}

#[test]
fn extent_knob_parser_is_default_off_and_only_accepts_documented_aliases() {
    assert_eq!(PosnObjectExtentMode::parse(None), PosnObjectExtentMode::Off);
    for value in ["off", "", " ", "unknown", "2", "verify", "gnu"] {
        assert_eq!(
            PosnObjectExtentMode::parse(Some(OsStr::new(value))),
            PosnObjectExtentMode::Off
        );
    }
    for value in ["on", "1", "true", "yes", "  ON  ", "True"] {
        assert_eq!(
            PosnObjectExtentMode::parse(Some(OsStr::new(value))),
            PosnObjectExtentMode::On
        );
    }
}

#[test]
#[cfg(unix)]
fn extent_knob_non_unicode_is_explicit_baseline_without_environment_mutation() {
    use std::os::unix::ffi::OsStrExt;
    assert_eq!(
        PosnObjectExtentMode::parse(Some(OsStr::from_bytes(b"on\xff"))),
        PosnObjectExtentMode::Off
    );
}

#[test]
fn live_walk_indices_read_retained_matrix_even_when_geometry_and_text_differ() {
    let snapshot = retained();
    for role in [
        DisplayPointRole::Glyph,
        DisplayPointRole::InsertionBoundary,
        DisplayPointRole::SyntheticBoundary,
        DisplayPointRole::OverlaidMarker,
    ] {
        let query = point(2, 1, role);
        let before = query.clone();
        assert_eq!(
            retained_posn_extent(Some(&snapshot), query.row, query.col, GlyphArea::Text)
                .dimensions(),
            (1, 0)
        );
        assert_eq!(query, before);
        assert_eq!(
            retained_posn_extent(None, query.row, query.col, GlyphArea::Text),
            PosnObjectExtent::Undrawn
        );
    }
    // Both queries may describe the same live point. Matrix horizontal index
    // governs object extent; its physical rectangle and buffer identity do not.
    assert_eq!(
        retained_posn_extent(Some(&snapshot), 2, 3, GlyphArea::Text).dimensions(),
        (0, 1)
    );
    assert_eq!(
        retained_posn_extent(Some(&snapshot), 3, 0, GlyphArea::Text).dimensions(),
        (0, 0)
    );
    assert_eq!(snapshot.text_body_position(2, 2), (0, 0));
    assert_eq!(
        retained_posn_extent(Some(&snapshot), 2, 4, GlyphArea::Text).dimensions(),
        (0, 1)
    );
    // Changing live top chrome before redisplay changes the queried matrix
    // index; the old accepted matrix is intentionally still the authority.
    assert_eq!(
        retained_posn_extent(Some(&snapshot), 1, 4, GlyphArea::Text).dimensions(),
        (1, 0)
    );
}

#[test]
fn every_point_role_survives_compact_wide_and_shared_row_relocation() {
    assert_eq!(std::mem::size_of::<PointCell>(), 16);
    for role in [
        DisplayPointRole::Glyph,
        DisplayPointRole::OverlaidMarker,
        DisplayPointRole::InsertionBoundary,
        DisplayPointRole::SyntheticBoundary,
    ] {
        let original = point(2, 1, role);
        let compact = DisplayPointRow::from_points(vec![original.clone()]);
        assert!(compact.is_compact());
        assert_eq!(compact.points().next(), Some(original.clone()));
        let moved = compact.try_replaced_placement(5, 20, 10).unwrap();
        assert!(compact.shares_cells_with(&moved));
        let moved_point = moved.points().next().unwrap();
        assert_eq!(moved_point.role, role);
        assert_eq!(moved_point.buffer_pos, LispCharPos1::new(17));
        assert_eq!(
            (
                moved_point.row,
                moved_point.y,
                moved_point.width,
                moved_point.height
            ),
            (5, 20, 17, 19)
        );
        let mut exceptional = original;
        exceptional.col = i64::MAX;
        let wide = DisplayPointRow::from_points(vec![exceptional.clone()]);
        assert!(!wide.is_compact());
        assert_eq!(wide.points().next(), Some(exceptional));
    }
}

#[test]
fn captured_active_presentation_keeps_its_matrix_when_a_new_snapshot_replaces_it() {
    let accepted = crate::window::WindowPresentationSnapshot::live(retained());
    let old = accepted.clone();
    let mut next = accepted;
    next.display_snapshot_mut().posn_matrix = Some(Arc::new(PosnMatrixSnapshot {
        rows: vec![PosnMatrixRow {
            enabled: true,
            y: 0,
            height: 1,
            areas: [0, 0, 0],
        }],
    }));
    assert_eq!(
        retained_posn_extent(Some(old.display_snapshot()), 0, 0, GlyphArea::Text).dimensions(),
        (1, 0)
    );
    assert_eq!(
        retained_posn_extent(Some(next.display_snapshot()), 0, 0, GlyphArea::Text).dimensions(),
        (0, 1)
    );
}

#[test]
fn insertion_and_synthetic_positions_keep_precedence_over_overlaid_markers() {
    use crate::window::DisplayPointRows;
    for role in [
        DisplayPointRole::Glyph,
        DisplayPointRole::InsertionBoundary,
        DisplayPointRole::SyntheticBoundary,
    ] {
        let ordinary = point(2, 1, role);
        let marker = point(0, 0, DisplayPointRole::OverlaidMarker);
        let points = vec![marker, ordinary.clone()];
        let flat = WindowDisplaySnapshot {
            points: points.clone(),
            ..Default::default()
        };
        assert_eq!(
            flat.point_for_buffer_pos(LispCharPos1::new(7)),
            Some(ordinary.clone())
        );
        let compact = WindowDisplaySnapshot {
            point_rows: Some(DisplayPointRows::from_points(points)),
            ..Default::default()
        };
        assert_eq!(
            compact.point_for_buffer_pos(LispCharPos1::new(7)),
            Some(ordinary)
        );
    }
}
