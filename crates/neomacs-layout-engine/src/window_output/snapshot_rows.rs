//! Evaluator-independent finalization of row and hit-position snapshots.
//!
//! These rows carry no window publication rights, freshness certificate or
//! rooted chrome strings. The evaluator attaches those when accepting output.

use neovm_core::window::{
    DisplayPointRows, DisplayPointSnapshot, DisplayRowSnapshot, PresentedBodyRowSnapshot,
};

pub(crate) struct PreparedWindowRows {
    pub(crate) points: Vec<DisplayPointSnapshot>,
    pub(crate) point_rows: Option<DisplayPointRows>,
    pub(crate) rows: Vec<DisplayRowSnapshot>,
    pub(crate) body_rows: Vec<PresentedBodyRowSnapshot>,
}

impl PreparedWindowRows {
    pub(super) fn new(
        points: Vec<DisplayPointSnapshot>,
        rows: Vec<DisplayRowSnapshot>,
        text_row_base: i64,
        body_origin_y: i64,
    ) -> Self {
        Self::with_point_rows(points, None, rows, text_row_base, body_origin_y)
    }

    pub(super) fn with_point_rows(
        mut points: Vec<DisplayPointSnapshot>,
        mut point_rows: Option<DisplayPointRows>,
        mut rows: Vec<DisplayRowSnapshot>,
        text_row_base: i64,
        body_origin_y: i64,
    ) -> Self {
        if let Some(frozen) = &mut point_rows {
            if !points.is_empty() {
                frozen
                    .rows
                    .extend(DisplayPointRows::from_points(std::mem::take(&mut points)).rows);
            }
            frozen.rows.sort_by_key(|row| row.row());
        }
        points.sort_by_key(|point| (point.buffer_pos, point.row, point.col, point.x));
        rows.sort_by_key(|row| row.row);
        let mut body_rows: Vec<PresentedBodyRowSnapshot> = Vec::with_capacity(rows.len());
        let mut push = |point: &DisplayPointSnapshot| {
            if body_rows
                .last()
                .is_some_and(|row| row.output_row == point.row)
            {
                return;
            }
            body_rows.push(PresentedBodyRowSnapshot {
                output_row: point.row,
                body_row: point.row.saturating_sub(text_row_base),
                body_y: point.y.saturating_sub(body_origin_y),
            });
        };
        match &point_rows {
            Some(frozen) => {
                // Each descriptor already knows its first source-ordered point.
                // Publication visits rows, without decoding every character.
                for row in &frozen.rows {
                    if let Some(point) = row.points().next() {
                        push(&point);
                    }
                }
            }
            None => {
                // Preserve the legacy source traversal and its first-point
                // convention when storage is flat.
                for point in &points {
                    push(point);
                }
            }
        }
        body_rows.sort_by_key(|row| row.output_row);
        body_rows.dedup_by_key(|row| row.output_row);
        Self {
            points,
            point_rows,
            rows,
            body_rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neovm_core::buffer::LispCharPos1;
    use neovm_core::window::DisplayPointRole;

    #[test]
    fn worker_finalization_preserves_source_order_and_revisited_row_geometry() {
        fn require_send_sync<T: Send + Sync + 'static>() {}
        require_send_sync::<PreparedWindowRows>();
        let points: Vec<_> = [(4, 3, 64), (2, 2, 48), (1, 3, 60), (3, 2, 48)]
            .into_iter()
            .map(|(pos, row, y)| DisplayPointSnapshot {
                role: DisplayPointRole::Glyph,
                buffer_pos: LispCharPos1::new(pos),
                x: 0,
                y,
                width: 8,
                height: 16,
                row,
                col: pos,
            })
            .collect();
        // This is the previous canonical algorithm: first source-ordered
        // point wins when several positions describe the same output row.
        let mut ordered = points.clone();
        ordered.sort_by_key(|point| (point.buffer_pos, point.row, point.col, point.x));
        let mut expected: Vec<_> = ordered
            .iter()
            .map(|point| PresentedBodyRowSnapshot {
                output_row: point.row,
                body_row: point.row.saturating_sub(2),
                body_y: point.y.saturating_sub(16),
            })
            .collect();
        expected.sort_by_key(|row| row.output_row);
        expected.dedup_by_key(|row| row.output_row);
        let result = std::thread::spawn(move || PreparedWindowRows::new(points, Vec::new(), 2, 16))
            .join()
            .unwrap();
        assert_eq!(result.points, ordered);
        assert_eq!(result.body_rows, expected);
        assert_eq!(result.body_rows.len(), 2);
        assert_eq!(result.body_rows[1].body_y, 44);
    }
}
