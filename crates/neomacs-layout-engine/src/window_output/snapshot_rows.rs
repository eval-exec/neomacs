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
#[path = "tests/snapshot_rows_test.rs"]
mod tests;
