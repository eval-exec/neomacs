//! Attempt-owned candidate state. Placement and face admission can inspect
//! immutable old rows; buffer-position materialization is a consuming step.
//! Each attempt owns its vector and immutable MatrixRow handles. No shared
//! mutable state, Lisp owner, cache, or invalidation lifetime is introduced.

use super::{GlyphRow, MatrixRow};
use crate::types::LayoutCharPos0;
use std::num::NonZeroI64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeferShiftMode {
    Off,
    On,
}

impl DeferShiftMode {
    fn parse(value: Option<&std::ffi::OsStr>) -> Self {
        if value.is_none() {
            return Self::On;
        }
        match value.and_then(std::ffi::OsStr::to_str) {
            Some(value)
                if matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "on" | "1" | "true" | "yes"
                ) =>
            {
                Self::On
            }
            _ => Self::Off,
        }
    }
}

#[inline]
pub(super) fn mode() -> DeferShiftMode {
    static MODE: std::sync::OnceLock<DeferShiftMode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        DeferShiftMode::parse(std::env::var_os("NEOMACS_EDIT_SYNC_DEFER_SHIFT").as_deref())
    })
}

#[derive(Clone, Debug)]
pub(super) struct CandidateRows {
    stage: CandidateStage,
}

static_assertions::assert_impl_all!(MatrixRow: Send, Sync);
static_assertions::assert_impl_all!(CandidateRows: Send, Sync);

/// A nonzero change measured in buffer characters, not bytes or pixels.
#[derive(Clone, Copy, Debug)]
struct BufferCharShift(NonZeroI64);

#[derive(Clone, Debug)]
enum CandidateStage {
    Ready(Vec<(usize, MatrixRow)>),
    Deferred {
        rows: Vec<(usize, MatrixRow)>,
        from: LayoutCharPos0,
        shift: BufferCharShift,
    },
}

impl CandidateRows {
    pub(super) fn ready(rows: Vec<(usize, MatrixRow)>) -> Self {
        Self {
            stage: CandidateStage::Ready(rows),
        }
    }

    /// Keep the original eager iterator/collection for OFF and zero delta.
    /// Deferred candidates share immutable rows only, with numeric edit data;
    /// they do not retain a mutable buffer or borrow another attempt's state.
    pub(super) fn prepare<'a>(
        candidates: impl Iterator<Item = (usize, &'a MatrixRow)>,
        dirty_start: i64,
        delta: i64,
        mode: DeferShiftMode,
    ) -> Self {
        match (mode, NonZeroI64::new(delta)) {
            (DeferShiftMode::Off, _) | (DeferShiftMode::On, None) => Self::ready(
                candidates
                    .map(|(index, row)| {
                        (index, super::shift_row_positions(row, dirty_start, delta))
                    })
                    .collect(),
            ),
            (DeferShiftMode::On, Some(delta)) => Self {
                stage: CandidateStage::Deferred {
                    rows: candidates
                        .map(|(index, row)| (index, row.clone()))
                        .collect(),
                    from: LayoutCharPos0::new(dirty_start),
                    shift: BufferCharShift(delta),
                },
            },
        }
    }

    /// Only placement is valid across both stages: indexes, y, enabled state
    /// and face IDs are unaffected by shifting buffer positions/cursor bits.
    pub(super) fn placement_rows(&self) -> &[(usize, MatrixRow)] {
        match &self.stage {
            CandidateStage::Ready(rows) | CandidateStage::Deferred { rows, .. } => rows,
        }
    }

    pub(super) fn face_rows(&self) -> impl Iterator<Item = &GlyphRow> {
        self.placement_rows().iter().map(|(_, row)| row.as_ref())
    }

    /// Select one admitted suffix before any deferred source positioning.
    pub(super) fn select_suffix(&mut self, first: super::prepared_suffix::EditSyncRowIndex) {
        let rows = match &mut self.stage {
            CandidateStage::Ready(rows) | CandidateStage::Deferred { rows, .. } => rows,
        };
        rows.retain(|(index, _)| {
            matches!(
                super::prepared_suffix::EditSyncRowIndex::new(*index).membership(first),
                super::prepared_suffix::SuffixMembership::Keep,
            )
        });
    }

    /// The sole gateway to rows in new buffer coordinates. Ready transfers
    /// its original vector and Arc handles unchanged, including Still reuse.
    pub(super) fn into_positioned(self) -> Vec<(usize, MatrixRow)> {
        match self.stage {
            CandidateStage::Ready(rows) => rows,
            CandidateStage::Deferred {
                mut rows,
                from,
                shift,
            } => {
                for (_, row) in &mut rows {
                    *row = super::shift_row_positions(row, from.get(), shift.0.get());
                }
                rows
            }
        }
    }

    #[cfg(test)]
    pub(super) fn ready_for_test_mut(&mut self) -> &mut Vec<(usize, MatrixRow)> {
        match &mut self.stage {
            CandidateStage::Ready(rows) => rows,
            CandidateStage::Deferred { .. } => panic!("test fixture must contain ready candidates"),
        }
    }
}

#[cfg(test)]
#[path = "../tests/edit_sync_candidate_rows_test.rs"]
mod tests;
