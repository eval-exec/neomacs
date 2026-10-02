//! Private numeric position storage for an immutable presented window.
//!
//! Deferred publications retain only shared point-row descriptors and numeric
//! mappings. Their OnceLock publishes one complete vector to concurrent readers;
//! cloning shares the immutable source and cache. No live window or Lisp value is
//! retained. Equality and Debug intentionally materialize the original vector.
//!
//! | Knob | Default | Gate |
//! | --- | --- | --- |
//! | `NEOMACS_PRESENT_GEOMETRY_LAZY=off\|on` | off | Defer the private presented position vector for shared point rows. |
//!
//! The mode is read once per process. Publications and their numeric caches may
//! be cloned and read concurrently; every caller observes the same completed
//! vector and no cache state participates in semantic equality.

use super::{GeometryError, PresentationPosition};
use crate::window::{
    DisplayPointRows, DisplayPointSnapshot, PresentedBodyRowSnapshot, WindowCursorSnapshot,
    WindowDisplaySnapshot,
};
use rustc_hash::FxHashMap as HashMap;
use std::sync::{Arc, OnceLock};

/// Numeric process-wide mode published by OnceLock; safe to copy across threads.
/// Contains no Lisp values, live window references, or mutable evaluator state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GeometryPositionsMode {
    Off,
    On,
}

impl GeometryPositionsMode {
    #[inline]
    pub(super) fn from_setting(setting: Option<&str>) -> Self {
        if setting.is_some_and(|value| {
            let value = value.trim();
            ["on", "1", "true", "yes"]
                .iter()
                .any(|enabled| value.eq_ignore_ascii_case(enabled))
        }) {
            Self::On
        } else {
            Self::Off
        }
    }
}

#[inline]
pub(super) fn geometry_positions_mode() -> GeometryPositionsMode {
    static MODE: OnceLock<GeometryPositionsMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        GeometryPositionsMode::from_setting(
            std::env::var("NEOMACS_PRESENT_GEOMETRY_LAZY")
                .ok()
                .as_deref(),
        )
    })
}

/// Owned numeric positions or a shared immutable source. Clones may be read
/// concurrently; deferred clones share a OnceLock-published complete vector.
/// Neither representation stores Lisp values, evaluator state, or live windows.
#[derive(Clone)]
pub(super) enum PresentationPositions {
    Eager(Vec<PresentationPosition>),
    Deferred(Arc<DeferredPositions>),
}

/// Immutable numeric descriptors and maps with one thread-safe vector cache.
/// Concurrent readers publish once; no Lisp state or snapshot object is retained.
pub(super) struct DeferredPositions {
    rows: DisplayPointRows,
    body_rows: HashMap<i64, PresentedBodyRowSnapshot>,
    row_heights: HashMap<i64, i64>,
    positions: OnceLock<Vec<PresentationPosition>>,
}

impl PresentationPositions {
    #[inline]
    pub(super) fn from_snapshot(
        snapshot: &WindowDisplaySnapshot,
        body_rows: HashMap<i64, PresentedBodyRowSnapshot>,
        row_heights: HashMap<i64, i64>,
        mode: GeometryPositionsMode,
    ) -> Result<Self, GeometryError> {
        if mode == GeometryPositionsMode::On {
            if let Some(rows) = snapshot.point_rows.as_ref() {
                // The merge ranks equal source positions by descriptor Vec
                // index, not visual row id. Ignore empty descriptors, exactly
                // as iter_points does, and report its first missing mapping.
                let first_missing = rows
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| !body_rows.contains_key(&row.row()))
                    .filter_map(|(index, row)| {
                        row.min_buffer_position()
                            .map(|position| ((position, index), row.row()))
                    })
                    .min_by_key(|(rank, _)| *rank);
                if let Some((_, output_row)) = first_missing {
                    return Err(GeometryError::MissingBodyRow {
                        window: snapshot.window_id,
                        output_row,
                    });
                }
                return Ok(Self::Deferred(Arc::new(DeferredPositions {
                    rows: rows.clone(),
                    body_rows,
                    row_heights,
                    positions: OnceLock::new(),
                })));
            }
        }
        // Legacy flat snapshots and the OFF arm preserve their exact original
        // traversal and eager error order, including unsorted compatibility data.
        snapshot
            .iter_points()
            .map(|point| {
                let body_row = body_rows
                    .get(&point.row)
                    .ok_or(GeometryError::MissingBodyRow {
                        window: snapshot.window_id,
                        output_row: point.row,
                    })?;
                Ok(project_point(point, body_row, &row_heights))
            })
            .collect::<Result<Vec<_>, GeometryError>>()
            .map(Self::Eager)
    }

    #[inline]
    pub(super) fn as_slice(&self) -> &[PresentationPosition] {
        match self {
            Self::Eager(points) => points,
            Self::Deferred(source) => source.positions.get_or_init(|| {
                source
                    .rows
                    .iter_points()
                    .map(|point| {
                        let body_row = &source.body_rows[&point.row];
                        project_point(point, body_row, &source.row_heights)
                    })
                    .collect()
            }),
        }
    }

    #[inline]
    pub(super) fn cursor_dimensions(
        &self,
        body_row: i64,
        col: i64,
        physical: Option<&WindowCursorSnapshot>,
    ) -> Option<(i64, i64)> {
        match self {
            Self::Eager(points) => {
                let point = points
                    .iter()
                    .find(|point| point.body_row == body_row && point.col == col);
                let width = physical
                    .map(|cursor| cursor.width)
                    .or_else(|| point.map(|p| p.width))?;
                let height = physical
                    .map(|cursor| cursor.height)
                    .or_else(|| point.map(|p| p.height))?;
                Some((width, height))
            }
            Self::Deferred(source) => {
                if let Some(cursor) = physical {
                    return Some((cursor.width, cursor.height));
                }
                // Preserve first canonical source match, including markers and
                // several output rows mapped to the same body row. No glyph
                // preference or visual-row sort is allowed here.
                source
                    .rows
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| {
                        source
                            .body_rows
                            .get(&row.row())
                            .is_some_and(|mapped| mapped.body_row == body_row)
                    })
                    .filter_map(|(row_index, row)| {
                        row.points()
                            .enumerate()
                            .find(|(_, point)| point.col == col)
                            .map(|(source_index, point)| {
                                (
                                    (point.buffer_pos, row_index, source_index),
                                    (point.width, point.height),
                                )
                            })
                    })
                    .min_by_key(|(rank, _)| *rank)
                    .map(|(_, dimensions)| dimensions)
            }
        }
    }

    #[cfg(test)]
    #[inline]
    pub(super) fn is_materialized(&self) -> bool {
        match self {
            Self::Eager(_) => true,
            Self::Deferred(source) => source.positions.get().is_some(),
        }
    }
}

#[inline]
fn project_point(
    point: DisplayPointSnapshot,
    body_row: &PresentedBodyRowSnapshot,
    row_heights: &HashMap<i64, i64>,
) -> PresentationPosition {
    PresentationPosition {
        buffer_pos: point.buffer_pos,
        x: point.x,
        body_y: body_row.body_y,
        width: point.width,
        height: point.height,
        // Compatibility snapshots can contain only points. Real publications
        // retain canonical mixed-font, raised-text and line-spacing extents.
        row_height: row_heights.get(&point.row).copied().unwrap_or(point.height),
        body_row: body_row.body_row,
        col: point.col,
    }
}

impl PartialEq for PresentationPositions {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}
impl Eq for PresentationPositions {}
impl std::fmt::Debug for PresentationPositions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_slice().fmt(f)
    }
}
