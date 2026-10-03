//! Numeric accepted TTY frame partitions for canonical position queries.
//!
//! GNU dispnew.c build_frame_matrix_from_leaf_window writes the complete
//! desired partition: term appends have (1,0) metrics and space_glyph holes
//! have (0,0). A current contribution copies its complete numeric partition.
//! Source rectangles and terminal erase pixels are never numeric authority.
//! Initial capture supports full-frame-width vertical partitions; horizontal
//! accepted borders and unproved reuse sources stay explicitly Undrawn.

use crate::frame_glyphs::GlyphRowRole;
use crate::glyph_matrix::{FrameDisplayState, GlyphArea, GlyphProvenance, RowDamage};
use crate::posn_object_extent::{PosnObjectExtent, terminal_area_used_cells};
use std::sync::Arc;

/// Exact row completed by its exclusive frame mutator before publication.
/// Readers share immutable Arcs; cells contain no Lisp state or mutation lock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PosnFramePoolRow {
    pub enabled: bool,
    pub height: i64,
    /// Accepted FRAME used[TEXT_AREA], including zero-metric space-glyph fill.
    pub used_text: usize,
    pub cells: Arc<[PosnObjectExtent]>,
}

/// Numeric identity and allocation bounds of an accepted window producer.
/// An independent mutator may read this immutable record; IDs and integer
/// geometry authorize a typed row-copy operation, never cache Lisp values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PosnFramePoolPartition {
    pub window: i64,
    pub first_row: i64,
    pub row_count: usize,
    pub first_column: i64,
    pub column_count: usize,
}

/// Fully initialized pool owned by one exclusive accepted Frame mutator.
/// Replacement occurs at accepted publication; readers retain immutable Arcs.
/// There is no production TLS, shared mutable row or pointer-identity admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PosnFramePool {
    pub columns: usize,
    pub lines: usize,
    pub rows: Vec<PosnFramePoolRow>,
    pub partitions: Vec<PosnFramePoolPartition>,
}

/// Attempt-local numeric provenance inspected by the exclusive frame producer.
/// It carries no Lisp state and becomes immutable before pool publication.
#[derive(Clone, Copy)]
enum AcceptedRowOperation {
    New,
    Copy { source: usize },
    Unsupported,
}

impl PosnFramePool {
    /// Observe final producer row provenance after the accepted layout seals.
    /// New full-width partitions record appends and explicit zero-space fill;
    /// copies require the preceding same-window allocation and typed shift.
    pub fn from_terminal_frame(
        state: &FrameDisplayState,
        previous: Option<&Self>,
        character_width: impl Fn(char) -> usize + Copy,
    ) -> Self {
        let columns = state.frame_cols;
        let lines = state.frame_rows;
        let zero = PosnObjectExtent::Glyph {
            width: 0,
            height: 0,
        };
        let previous = previous.filter(|old| old.columns == columns && old.lines == lines);
        let mut rows = if let Some(old) = previous {
            old.rows.clone()
        } else {
            let cells: Arc<[PosnObjectExtent]> = Arc::from(vec![zero; columns]);
            (0..lines)
                .map(|_| PosnFramePoolRow {
                    enabled: false,
                    height: 0,
                    used_text: 0,
                    cells: Arc::clone(&cells),
                })
                .collect()
        };
        for row in &mut rows {
            row.enabled = false;
            row.used_text = 0;
        }
        let mut partitions = Vec::with_capacity(state.window_matrices.len());
        let cw = state.char_width.max(1.0);
        let ch = state.char_height.max(1.0);
        for entry in &state.window_matrices {
            let partition = PosnFramePoolPartition {
                window: entry.window_id.get(),
                first_row: (entry.pixel_bounds.y / ch).round() as i64,
                row_count: (entry.pixel_bounds.height / ch).round().max(0.0) as usize,
                first_column: (entry.pixel_bounds.x / cw).round() as i64,
                column_count: (entry.pixel_bounds.width / cw).round().max(0.0) as usize,
            };
            let full_width = partition.first_column == 0
                && partition.column_count == columns
                && (entry.text_pixel_bounds.x / cw).round() as i64 == 0
                && (entry.text_pixel_bounds.width / cw).round().max(0.0) as usize == columns;
            let mut eob = None;
            for (index, raw) in entry.matrix.rows.iter().enumerate() {
                if !raw.enabled {
                    continue;
                }
                let bounds = entry.row_pixel_bounds(raw.role);
                let frame_y = (bounds.y / ch).round() as i64 + index as i64;
                if raw.role == GlyphRowRole::Text {
                    let clip = entry.text_clip_bounds.unwrap_or(entry.text_pixel_bounds);
                    let top = (clip.y / ch).round() as i64;
                    let end = top + (clip.height / ch).ceil().max(0.0) as i64;
                    if !(top..end).contains(&frame_y) {
                        continue;
                    }
                }
                let Some(target) = usize::try_from(frame_y).ok().filter(|y| *y < lines) else {
                    continue;
                };
                let local = frame_y - partition.first_row;
                let operation = if full_width {
                    Self::row_operation(
                        previous,
                        &partition,
                        local,
                        entry.matrix.row_damage(index),
                        ch,
                    )
                } else {
                    AcceptedRowOperation::Unsupported
                };
                let row = &mut rows[target];
                match operation {
                    AcceptedRowOperation::New => {
                        let mut count = terminal_area_used_cells(
                            &raw.glyphs[GlyphArea::Text.index()],
                            character_width,
                        );
                        if raw.role == GlyphRowRole::Text && raw.ends_at_zv {
                            if !raw.glyphs[GlyphArea::Text.index()]
                                .iter()
                                .any(|g| g.provenance == GlyphProvenance::LineEnd)
                            {
                                count = count.saturating_add(1);
                            }
                        } else {
                            // xdisp extend_face_to_end_of_line emits real term
                            // glyphs for ordinary newline/status-line tails.
                            count = columns;
                        }
                        Self::new_row(row, columns, count, raw.height_px.round().max(1.0) as i64);
                    }
                    AcceptedRowOperation::Copy { source } => {
                        *row = previous.unwrap().rows[source].clone()
                    }
                    AcceptedRowOperation::Unsupported => row.enabled = false,
                }
                if raw.role == GlyphRowRole::Text && raw.ends_at_zv {
                    eob = Some((frame_y, entry.matrix.row_damage(index)));
                }
            }
            if let Some((last_eob, damage)) = eob {
                // Main can omit undisplayed EOB filler rows. GNU's same body
                // iterator contributes each with one append and zero-space
                // fill, or with the typed reused body-row copy operation.
                let clip = entry.text_clip_bounds.unwrap_or(entry.text_pixel_bounds);
                let end = (clip.y / ch).round() as i64 + (clip.height / ch).ceil().max(0.0) as i64;
                for y in last_eob.saturating_add(1)..end.min(lines as i64) {
                    let Ok(target) = usize::try_from(y) else {
                        continue;
                    };
                    let operation = if full_width {
                        Self::row_operation(
                            previous,
                            &partition,
                            y - partition.first_row,
                            damage,
                            ch,
                        )
                    } else {
                        AcceptedRowOperation::Unsupported
                    };
                    match operation {
                        AcceptedRowOperation::New => {
                            Self::new_row(&mut rows[target], columns, 1, ch.round().max(1.0) as i64)
                        }
                        AcceptedRowOperation::Copy { source } => {
                            rows[target] = previous.unwrap().rows[source].clone()
                        }
                        AcceptedRowOperation::Unsupported => rows[target].enabled = false,
                    }
                }
            }
            partitions.push(partition);
        }
        Self {
            columns,
            lines,
            rows,
            partitions,
        }
    }

    fn row_operation(
        previous: Option<&Self>,
        partition: &PosnFramePoolPartition,
        local: i64,
        damage: RowDamage,
        char_height: f32,
    ) -> AcceptedRowOperation {
        if !(0..partition.row_count as i64).contains(&local) {
            return AcceptedRowOperation::Unsupported;
        }
        if matches!(damage, RowDamage::New) {
            return AcceptedRowOperation::New;
        }
        let Some(previous) = previous else {
            return AcceptedRowOperation::Unsupported;
        };
        let Some(old) = previous.partitions.iter().find(|old| {
            old.window == partition.window
                && old.first_row == partition.first_row
                && old.first_column == partition.first_column
                && old.column_count == partition.column_count
                && old.row_count == partition.row_count
        }) else {
            return AcceptedRowOperation::Unsupported;
        };
        let shift = match damage {
            RowDamage::Reused => 0,
            RowDamage::ReusedShifted { dvpos } => {
                let cells = dvpos.0 / char_height;
                if !cells.is_finite() || cells.fract() != 0.0 {
                    return AcceptedRowOperation::Unsupported;
                }
                cells as i64
            }
            RowDamage::New => unreachable!(),
        };
        let Some(source_local) = local
            .checked_sub(shift)
            .filter(|row| (0..old.row_count as i64).contains(row))
        else {
            return AcceptedRowOperation::Unsupported;
        };
        let Some(source) = old
            .first_row
            .checked_add(source_local)
            .and_then(|y| usize::try_from(y).ok())
            .filter(|y| previous.rows.get(*y).is_some_and(|row| row.enabled))
        else {
            return AcceptedRowOperation::Unsupported;
        };
        AcceptedRowOperation::Copy { source }
    }

    fn new_row(row: &mut PosnFramePoolRow, columns: usize, append_count: usize, height: i64) {
        let count = append_count.min(columns);
        let appended = PosnObjectExtent::Glyph {
            width: 1,
            height: 0,
        };
        let filled = PosnObjectExtent::Glyph {
            width: 0,
            height: 0,
        };
        // Compare final producer writes before COW, never clear and restore
        // identical cells merely to model an operation with unchanged output.
        if row.cells[..count].iter().any(|cell| *cell != appended)
            || row.cells[count..].iter().any(|cell| *cell != filled)
        {
            let cells = Arc::make_mut(&mut row.cells);
            cells[..count].fill(appended);
            cells[count..].fill(filled);
        }
        row.enabled = true;
        row.height = height;
        row.used_text = columns;
    }

    /// Owner reuse is allowed only after all new accepted observations agree.
    pub fn retains_same_observations(&self, old: &Self) -> bool {
        self.columns == old.columns
            && self.lines == old.lines
            && self.partitions == old.partitions
            && self.rows.len() == old.rows.len()
            && self.rows.iter().zip(&old.rows).all(|(row, old)| {
                row.enabled == old.enabled
                    && row.height == old.height
                    && row.used_text == old.used_text
                    && Arc::ptr_eq(&row.cells, &old.cells)
            })
    }

    /// Borrowed fake-current-matrix partition read. The live leaf supplies
    /// area used counts; exact numeric values and enabled metadata stay accepted.
    pub fn at_partition(
        &self,
        first_row: i64,
        row_count: usize,
        first_columns: [i64; GlyphArea::COUNT],
        used: [usize; GlyphArea::COUNT],
        row: i64,
        column: i64,
        area: GlyphArea,
    ) -> PosnObjectExtent {
        let Some(local) = usize::try_from(row).ok().filter(|row| *row < row_count) else {
            return PosnObjectExtent::Undrawn;
        };
        let Some(retained) = first_row
            .checked_add(local as i64)
            .and_then(|y| usize::try_from(y).ok())
            .and_then(|y| self.rows.get(y))
        else {
            return PosnObjectExtent::Undrawn;
        };
        if !retained.enabled {
            return PosnObjectExtent::Undrawn;
        }
        let area = area.index();
        if usize::try_from(column)
            .ok()
            .is_none_or(|column| column >= used[area])
        {
            return PosnObjectExtent::RowFallback {
                height: retained.height,
            };
        }
        first_columns[area]
            .checked_add(column)
            .and_then(|x| usize::try_from(x).ok())
            .and_then(|x| retained.cells.get(x))
            .copied()
            .unwrap_or(PosnObjectExtent::Undrawn)
    }
}

/// Independently owned query view sharing immutable accepted numeric cells.
#[derive(Clone, Debug)]
pub struct PosnFramePoolSlice {
    pub pool: Arc<PosnFramePool>,
    pub first_row: i64,
    pub row_count: usize,
    pub first_columns: [i64; GlyphArea::COUNT],
    pub used: [usize; GlyphArea::COUNT],
}
impl PosnFramePoolSlice {
    pub fn at(&self, row: i64, column: i64, area: GlyphArea) -> PosnObjectExtent {
        self.pool.at_partition(
            self.first_row,
            self.row_count,
            self.first_columns,
            self.used,
            row,
            column,
            area,
        )
    }
}

#[cfg(test)]
#[path = "tests/frame_pool_test.rs"]
mod tests;
