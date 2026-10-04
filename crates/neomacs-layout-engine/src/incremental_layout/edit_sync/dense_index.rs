//! Attempt-owned numeric indexes for edit synchronization only.
//! Immutable process policy is shared through OnceLock. Every range/map is
//! stack-owned by one layout attempt and contains no Lisp handle, heap pointer,
//! cache, or shared mutation; independent mutators may use it concurrently.
use super::{EditSyncInstall, EditSyncPlan, MatrixRow, RetainedWindowMatrix};
use neovm_core::buffer::position::LispCharPos1;

/// Pure startup selector parser; every absent/empty/invalid/nonUnicode value is OFF.
#[cold]
#[inline(never)]
fn parse(value: Option<&std::ffi::OsStr>) -> bool {
    value
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "on" | "1" | "true" | "yes"
            )
        })
}

/// Immutable process flag, initialized before publication by OnceLock.
/// Numeric test policy is local to its owning test thread; no Lisp TLS exists.
#[inline]
pub(super) fn enabled() -> bool {
    #[cfg(test)]
    if let Some(value) = super::dense_index_test_support::forced() {
        return value;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| parse(std::env::var_os("NEOMACS_EDIT_SYNC_DENSE_INDEX").as_deref()))
}

/// A proved nonempty ascending sequence of consecutive signed row indexes.
/// This attempt-owned value admits no gaps, duplicates, casts that wrap, or
/// unrepresentable exclusive endpoints. It publishes no shared state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DenseRows {
    first: i64,
    end: i64,
}
impl DenseRows {
    #[inline]
    fn from_indices(indices: impl IntoIterator<Item = usize>) -> Option<Self> {
        let mut indices = indices.into_iter();
        let first = i64::try_from(indices.next()?).ok()?;
        let mut end = first.checked_add(1)?;
        for index in indices {
            if i64::try_from(index).ok()? != end {
                return None;
            }
            end = end.checked_add(1)?;
        }
        Some(Self { first, end })
    }
    #[inline]
    fn contains(self, row: i64) -> bool {
        self.first <= row && row < self.end
    }
}

/// A proved mapping of the actual surviving old row keys to old+offset.
/// Empty survivor maps are explicit. Snapshot duplicates remain independent
/// vector entries in their original order; this value is attempt-owned numeric
/// state, never a cache or a frame/Context field.
#[derive(Clone, Copy, Debug)]
pub(super) struct DenseRowMap {
    range: Option<DenseRows>,
    offset: i64,
}
impl DenseRowMap {
    #[inline]
    fn at(self, row: i64) -> Option<i64> {
        self.range.filter(|range| range.contains(row))?;
        // Every admitted endpoint mapped successfully during the proof; keep
        // checked arithmetic here too so an unrelated caller cannot panic.
        row.checked_add(self.offset)
    }
}

/// Prove the exact keys the original installer would insert. A clipped interior
/// row can make survivors sparse even when all candidate indexes were dense.
/// Refusal consumes no rows and returns to the unchanged original installer.
#[inline]
pub(super) fn installation_map(
    rows: &[(usize, MatrixRow)],
    dvpos: i64,
    dy: f32,
    shift_y: bool,
    bottom_y: f32,
    index_limit: usize,
) -> Option<DenseRowMap> {
    let mut first = None;
    let mut end = None;
    for (old_index, row) in rows {
        let old = i64::try_from(*old_index).ok()?;
        let new = old.checked_add(dvpos)?;
        let Ok(new_index) = usize::try_from(new) else {
            continue;
        };
        let new_y = row.pixel_y + if shift_y { dy } else { 0.0 };
        if new_y >= bottom_y - 0.5 || new_index >= index_limit {
            continue;
        }
        if end.is_some_and(|expected| old != expected) {
            return None;
        }
        first.get_or_insert(old);
        end = Some(old.checked_add(1)?);
    }
    Some(DenseRowMap {
        range: first.zip(end).map(|(first, end)| DenseRows { first, end }),
        offset: dvpos,
    })
}

/// Build an edit plan only after its original pointer/continuation/stop gates.
/// The strictly consecutive candidate proof replaces the membership set only;
/// row shifts and snapshot traversal/order remain the original operations.
#[inline]
pub(super) fn plan(
    prev: &RetainedWindowMatrix,
    candidates: &[(usize, &MatrixRow)],
    dirty_start: i64,
    delta: i64,
    stop_charpos: usize,
) -> Option<EditSyncPlan> {
    let indices = DenseRows::from_indices(candidates.iter().map(|(index, _)| *index))?;
    let (first_index, first_row) = candidates[0];
    let rows = candidates
        .iter()
        .map(|&(index, row)| (index, super::shift_row_positions(row, dirty_start, delta)))
        .collect();
    let shift = |p: LispCharPos1| {
        LispCharPos1::from_one_based_usize((p.to_one_based_usize() as i64 + delta) as usize)
    };
    let row_snapshots = prev
        .display_snapshot
        .rows
        .iter()
        .filter(|row| indices.contains(row.row))
        .map(|row| {
            let mut row = row.clone();
            row.start_buffer_pos = row.start_buffer_pos.map(shift);
            row.end_buffer_pos = row.end_buffer_pos.map(shift);
            row
        })
        .collect();
    let points = prev
        .display_snapshot
        .points
        .iter()
        .filter(|point| indices.contains(point.row))
        .map(|point| {
            let mut point = point.clone();
            point.buffer_pos = shift(point.buffer_pos);
            point
        })
        .collect();
    let point_rows = prev.display_snapshot.point_rows.as_ref().map(|points| {
        neovm_core::window::DisplayPointRows {
            rows: points
                .rows
                .iter()
                .filter(|row| indices.contains(row.row()))
                .map(|row| row.replaced_placement(row.row(), row.y(), delta))
                .collect(),
        }
    });
    #[cfg(test)]
    super::dense_index_test_support::note_dense_plan();
    Some(EditSyncPlan {
        stop_charpos,
        first_unchanged_index: first_index,
        first_unchanged_y: first_row.pixel_y,
        rows,
        row_snapshots,
        points,
        point_rows,
    })
}

/// Install a plan whose surviving-key mapping has just been proved. This
/// consumes the same owned vectors as the original hash installer. Numeric
/// proof admits no duplicate survivor keys, preserving the original map's
/// last-write semantics by falling back whenever such a key occurs.
#[inline]
pub(super) fn install(
    plan: EditSyncPlan,
    kept: DenseRowMap,
    dvpos: i64,
    dy: f32,
    shift_y: bool,
    bottom_y: f32,
    index_limit: usize,
) -> EditSyncInstall {
    let dy_px = dy.round() as i64;
    let mut rows = Vec::with_capacity(plan.rows.len());
    for (old_index, row) in plan.rows {
        let new_y = row.pixel_y + if shift_y { dy } else { 0.0 };
        let Ok(new_index) = usize::try_from(old_index as i64 + dvpos) else {
            continue;
        };
        if new_y >= bottom_y - 0.5 || new_index >= index_limit {
            continue;
        }
        let row = if shift_y {
            let mut shifted = super::GlyphRow::clone(&row);
            shifted.pixel_y = new_y;
            shifted.keep_appearance_of(&row);
            MatrixRow::new(shifted)
        } else {
            row
        };
        rows.push((new_index, row));
    }
    let row_snapshots = plan
        .row_snapshots
        .into_iter()
        .filter_map(|mut snapshot| {
            let new_row = kept.at(snapshot.row)?;
            snapshot.row = new_row;
            if shift_y {
                snapshot.y += dy_px;
            }
            Some(snapshot)
        })
        .collect();
    let points = plan
        .points
        .into_iter()
        .filter_map(|mut point| {
            let new_row = kept.at(point.row)?;
            point.row = new_row;
            if shift_y {
                point.y += dy_px;
            }
            Some(point)
        })
        .collect();
    let point_rows = plan
        .point_rows
        .map(|points| neovm_core::window::DisplayPointRows {
            rows: points
                .rows
                .into_iter()
                .filter_map(|row| {
                    let new_row = kept.at(row.row())?;
                    Some(row.replaced_placement(
                        new_row,
                        row.y() + if shift_y { dy_px } else { 0 },
                        0,
                    ))
                })
                .collect(),
        });
    #[cfg(test)]
    super::dense_index_test_support::note_dense_install();
    EditSyncInstall {
        rows,
        row_snapshots,
        points,
        point_rows,
        dy: if shift_y { dy } else { 0.0 },
    }
}

#[cfg(test)]
#[path = "../tests/edit_sync_dense_index_policy_test.rs"]
mod policy_tests;
