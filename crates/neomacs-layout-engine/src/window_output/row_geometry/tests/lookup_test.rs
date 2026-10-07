//! Emitter lookup must keep pre-publication walk order when cells freeze.
use super::*;
use neovm_core::window::DisplayPointRole;

fn point(row: i64, col: i64, x: i64, role: DisplayPointRole) -> DisplayPointSnapshot {
    DisplayPointSnapshot {
        role,
        buffer_pos: LispCharPos1::new(7),
        x,
        y: row * 16,
        width: 8,
        height: 16,
        row,
        col,
    }
}

fn geometry(frozen: bool) -> WindowRowGeometry {
    let mut geometry = WindowRowGeometry::new(0, 0.0, 0.0);
    // Pure storage selection; the process-wide environment knob is untouched.
    geometry.point_rows = frozen.then(DisplayPointRows::default);
    geometry
}

fn close(geometry: &mut WindowRowGeometry, row: i64, points: &[DisplayPointSnapshot]) {
    geometry.begin_current_row_progress(Some(row as usize), row, 0, row * 16, 0);
    geometry.points.extend_from_slice(points);
    geometry.push_text_row((row * 16) as f32, 16.0, 12.0);
}

#[test]
fn closed_anchor_precedes_duplicate_on_unfinished_row() {
    for role in [DisplayPointRole::Glyph, DisplayPointRole::OverlaidMarker] {
        let closed = point(0, 3, 24, role);
        let current = point(1, 0, 0, DisplayPointRole::Glyph);
        for frozen in [false, true] {
            let mut geometry = geometry(frozen);
            close(&mut geometry, 0, std::slice::from_ref(&closed));
            geometry.points.push(current.clone());
            assert_eq!(
                geometry.point_for_buffer_pos(closed.buffer_pos),
                Some(closed.clone())
            );
        }
    }
}

#[test]
fn fresh_lookup_preserves_marker_and_duplicate_glyph_emission_order() {
    let first = point(0, 8, 64, DisplayPointRole::OverlaidMarker);
    let next = point(0, 1, 8, DisplayPointRole::Glyph);
    for frozen in [false, true] {
        let mut geometry = geometry(frozen);
        close(&mut geometry, 0, &[first.clone(), next.clone()]);
        assert_eq!(
            geometry.point_for_buffer_pos(first.buffer_pos),
            Some(first.clone())
        );
    }
    let first = point(0, 8, 64, DisplayPointRole::Glyph);
    for frozen in [false, true] {
        let mut geometry = geometry(frozen);
        close(&mut geometry, 0, &[first.clone(), next.clone()]);
        assert_eq!(
            geometry.point_for_buffer_pos(first.buffer_pos),
            Some(first.clone())
        );
    }
}

#[test]
fn reused_lookup_keeps_canonical_ties_without_glyph_preference() {
    let later = point(1, 0, 0, DisplayPointRole::Glyph);
    let earlier = point(0, 2, 16, DisplayPointRole::OverlaidMarker);
    let same_row_later = point(0, 9, 72, DisplayPointRole::Glyph);
    let mut canonical = vec![later, same_row_later, earlier.clone()];
    canonical.sort_by_key(|point| (point.buffer_pos, point.row, point.col, point.x));
    for seed in [false, true] {
        for frozen in [false, true] {
            let mut geometry = geometry(frozen);
            let (points, rows) = if frozen {
                (
                    Vec::new(),
                    Some(DisplayPointRows::from_points(canonical.clone())),
                )
            } else {
                (canonical.clone(), None)
            };
            if seed {
                geometry.seed_cursor_only_body(Vec::new(), points, rows);
            } else {
                geometry.push_reused_body(Vec::new(), points, rows);
            }
            assert_eq!(
                geometry.point_for_buffer_pos(earlier.buffer_pos),
                Some(earlier.clone())
            );
        }
    }
}

#[test]
fn fresh_and_reused_groups_keep_append_order() {
    let fresh = point(3, 5, 40, DisplayPointRole::Glyph);
    let reused = point(0, 0, 0, DisplayPointRole::OverlaidMarker);
    for frozen in [false, true] {
        let mut geometry = geometry(frozen);
        close(&mut geometry, 3, std::slice::from_ref(&fresh));
        let (points, rows) = if frozen {
            (
                Vec::new(),
                Some(DisplayPointRows::from_points(vec![reused.clone()])),
            )
        } else {
            (vec![reused.clone()], None)
        };
        geometry.push_reused_body(Vec::new(), points, rows);
        assert_eq!(
            geometry.point_for_buffer_pos(fresh.buffer_pos),
            Some(fresh.clone())
        );
    }
}
