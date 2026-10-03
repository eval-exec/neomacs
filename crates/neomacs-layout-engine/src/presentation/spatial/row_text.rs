//! Window-local hits from immutable C6 row cells.
//!
//! Sources own shared numeric snapshots. Rendering and input threads may query
//! them concurrently; `OnceLock` publishes each queried row's positions and x
//! index after initialization. No Lisp state or mutator-local cache is added.

use std::sync::{Arc, OnceLock};

use neomacs_display_protocol::{
    DisplayWindowId, FrameRect, PresentedHitError, PresentedTextPosition, Rect,
};
use neovm_core::window::{PresentedBodyRowSnapshot, WindowDisplaySnapshot};

/// Immutable row metadata and lazy row-local geometry for one shared window.
#[derive(Debug)]
pub(super) struct RowWindowText {
    window: DisplayWindowId,
    snapshot: Arc<WindowDisplaySnapshot>,
    text_body: Rect,
    rows: Vec<RowEntry>,
}

/// Numeric row selection, copied freely between concurrent immutable readers.
#[derive(Clone, Copy, Debug)]
enum RowKind {
    Glyph {
        index: usize,
        body: PresentedBodyRowSnapshot,
    },
    Fallback {
        index: usize,
    },
}

/// Bounds are immutable; concurrent readers initialize geometry through
/// `OnceLock`, which publishes the completed row before any reader sees it.
#[derive(Debug)]
struct RowEntry {
    top: f32,
    bottom: f32,
    prefix_bottom: f32,
    kind: RowKind,
    built: OnceLock<Result<RowPositions, PresentedHitError>>,
}

/// Owned numeric hit geometry, immutable after `RowEntry::built` publishes it.
/// It retains neither evaluator state nor a mutator thread identity.
#[derive(Debug)]
struct RowPositions {
    positions: Vec<PresentedTextPosition>,
    // The source key is the legacy canonical (buffer position, row, column, x).
    source_keys: Vec<(i64, i64, i64, i64)>,
    x_order: Vec<usize>,
    prefix_right: Vec<f32>,
}

impl RowPositions {
    fn new(positions: Vec<PresentedTextPosition>, source_keys: Vec<(i64, i64, i64, i64)>) -> Self {
        let mut x_order: Vec<_> = (0..positions.len()).collect();
        x_order.sort_by(|&a, &b| {
            positions[a]
                .bounds()
                .x()
                .total_cmp(&positions[b].bounds().x())
        });
        let mut right = f32::NEG_INFINITY;
        let prefix_right = x_order
            .iter()
            .map(|&index| {
                let bounds = positions[index].bounds();
                right = right.max(bounds.x() + bounds.width());
                right
            })
            .collect();
        Self {
            positions,
            source_keys,
            x_order,
            prefix_right,
        }
    }

    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        let mut end = self
            .x_order
            .partition_point(|&index| self.positions[index].bounds().x() <= x);
        let mut selected: Option<usize> = None;
        while end > 0 {
            end -= 1;
            if self.prefix_right[end] <= x {
                break;
            }
            let index = self.x_order[end];
            let bounds = self.positions[index].bounds();
            if x < bounds.x() + bounds.width()
                && y >= bounds.y()
                && y < bounds.y() + bounds.height()
            {
                selected = Some(selected.map_or(index, |previous| previous.min(index)));
            }
        }
        selected
    }
}

impl RowWindowText {
    /// Construction visits row descriptors only. Flat legacy/synthetic
    /// snapshots return `None`, retaining the old frame materializer.
    pub(super) fn new(
        window: DisplayWindowId,
        snapshot: Arc<WindowDisplaySnapshot>,
        text_body: Rect,
    ) -> Result<Option<Self>, PresentedHitError> {
        let Some(point_rows) = snapshot.point_rows.as_ref() else {
            return Ok(None);
        };
        // Integer cells always convert to finite f32 values. With a valid body
        // rectangle, clipping either discards them or produces valid geometry;
        // body-row existence is the remaining global check below.
        if !text_body.x.is_finite()
            || !text_body.y.is_finite()
            || !text_body.width.is_finite()
            || !text_body.height.is_finite()
            || text_body.x < 0.0
            || text_body.y < 0.0
            || text_body.width < 0.0
            || text_body.height < 0.0
            || !(text_body.x + text_body.width).is_finite()
            || !(text_body.y + text_body.height).is_finite()
        {
            return Err(PresentedHitError::InvalidTextPositionGeometry);
        }
        let mut rows = Vec::with_capacity(point_rows.rows.len() + snapshot.rows.len());
        for (index, row) in point_rows.rows.iter().enumerate() {
            if row.point_count() == 0 {
                continue;
            }
            let body = *snapshot.body_row_for_output_row(row.row()).ok_or(
                PresentedHitError::MissingBodyRow {
                    window,
                    output_row: row.row(),
                },
            )?;
            let top = (text_body.y + body.body_y as f32).max(text_body.y);
            let bottom = (text_body.y + body.body_y as f32 + row.max_height().max(1) as f32)
                .min(text_body.y + text_body.height);
            if text_body.width > 0.0 && bottom > top {
                rows.push(RowEntry {
                    top,
                    bottom,
                    prefix_bottom: bottom,
                    kind: RowKind::Glyph { index, body },
                    built: OnceLock::new(),
                });
            }
        }
        for (index, row) in snapshot.rows.iter().enumerate() {
            if row.start_buffer_pos.or(row.end_buffer_pos).is_none() {
                continue;
            }
            let (_, body_y) = snapshot.text_body_position(row.row, row.y);
            let top = (text_body.y + body_y as f32).max(text_body.y);
            let bottom = (text_body.y + body_y as f32 + row.height.max(1) as f32)
                .min(text_body.y + text_body.height);
            if text_body.width > 0.0 && bottom > top {
                rows.push(RowEntry {
                    top,
                    bottom,
                    prefix_bottom: bottom,
                    kind: RowKind::Fallback { index },
                    built: OnceLock::new(),
                });
            }
        }
        rows.sort_by(|a, b| a.top.total_cmp(&b.top));
        let mut bottom = f32::NEG_INFINITY;
        for row in &mut rows {
            bottom = bottom.max(row.bottom);
            row.prefix_bottom = bottom;
        }
        Ok(Some(Self {
            window,
            snapshot,
            text_body,
            rows,
        }))
    }

    pub(super) fn is_empty(&self) -> Option<bool> {
        // Row extents bound candidates but do not prove visibility in x (a
        // negative clipped row can also have taller fallbacks than its glyphs).
        // Avoid a false nonempty claim for unusual geometry.
        self.rows.is_empty().then_some(true)
    }

    pub(super) fn hit(
        &self,
        x: f32,
        y: f32,
    ) -> Result<Option<PresentedTextPosition>, PresentedHitError> {
        let mut end = self.rows.partition_point(|row| row.top <= y);
        let mut glyph = None;
        let mut fallback = None;
        while end > 0 {
            end -= 1;
            let row = &self.rows[end];
            if row.prefix_bottom <= y {
                break;
            }
            if row.bottom <= y {
                continue;
            }
            let positions = row
                .built
                .get_or_init(|| self.build_row(row.kind))
                .as_ref()
                .map_err(|error| *error)?;
            let Some(index) = positions.hit(x, y) else {
                continue;
            };
            let position = positions.positions[index];
            match row.kind {
                RowKind::Glyph {
                    index: row_index, ..
                } => {
                    let key = (positions.source_keys[index], row_index, index);
                    if glyph.as_ref().is_none_or(|(previous, _)| key < *previous) {
                        glyph = Some((key, position));
                    }
                }
                RowKind::Fallback { index: row_index } => {
                    let key = (row_index, index);
                    if fallback
                        .as_ref()
                        .is_none_or(|(previous, _)| key < *previous)
                    {
                        fallback = Some((key, position));
                    }
                }
            }
        }
        Ok(glyph
            .map(|(_, position)| position)
            .or_else(|| fallback.map(|(_, position)| position)))
    }

    fn build_row(&self, kind: RowKind) -> Result<RowPositions, PresentedHitError> {
        let mut positions = Vec::new();
        let mut source_keys = Vec::new();
        let point_rows = self
            .snapshot
            .point_rows
            .as_ref()
            .expect("row source owns row cells");
        match kind {
            RowKind::Glyph { index, body } => {
                for point in point_rows.rows[index].points() {
                    let raw_x = self.text_body.x + point.x as f32;
                    let raw_y = self.text_body.y + body.body_y as f32;
                    let left = raw_x.max(self.text_body.x);
                    let top = raw_y.max(self.text_body.y);
                    let right = (raw_x + point.width.max(1) as f32)
                        .min(self.text_body.x + self.text_body.width);
                    let bottom = (raw_y + point.height.max(1) as f32)
                        .min(self.text_body.y + self.text_body.height);
                    if right <= left || bottom <= top {
                        continue;
                    }
                    let bounds = FrameRect::new(left, top, right - left, bottom - top)
                        .map_err(|_| PresentedHitError::InvalidTextPositionGeometry)?;
                    positions.push(
                        PresentedTextPosition::new(
                            self.window,
                            bounds,
                            point.buffer_pos.as_i64(),
                            body.body_row,
                            point.col,
                        )
                        .with_point_role(
                            if neovm_core::window::posn_object_extent_mode().enabled() {
                                point.role
                            } else {
                                neomacs_display_protocol::posn_object_extent::PosnPointRole::Glyph
                            },
                        ),
                    );
                    source_keys.push((point.buffer_pos.as_i64(), point.row, point.col, point.x));
                }
            }
            RowKind::Fallback { index } => {
                let row = &self.snapshot.rows[index];
                let points: Vec<_> = point_rows
                    .row(row.row)
                    .map(|row| row.points_x_order().collect())
                    .unwrap_or_default();
                super::push_one_row_fallback_positions(
                    &mut positions,
                    self.window,
                    &self.snapshot,
                    self.text_body,
                    row,
                    &points,
                )?;
            }
        }
        Ok(RowPositions::new(positions, source_keys))
    }
}

#[cfg(test)]
#[path = "../tests/spatial_row_text_test.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/spatial_row_role_test.rs"]
mod canonical_role_tests;
