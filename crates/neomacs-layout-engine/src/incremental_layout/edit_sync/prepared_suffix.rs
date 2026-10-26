//! A single attempt-owned suffix and its admitted alternative frontiers.
//!
//! Frame face admission visits the union once. The source walk consumes this
//! preparation into exactly one selected plan; unselected prefix rows are
//! discarded before their buffer positions are materialized. No Lisp roots,
//! cache, TLS state or cross-mutator mutable owner is retained here.

use super::{
    EditReplayPositions, EditSyncInstall, EditSyncPlan, EditSyncReached, EditSyncStop, GlyphRow,
};
use crate::buffer_source::row_lifecycle::SourceLineRestart;
use crate::types::LayoutCharPos0;
use neovm_core::buffer::CharPos0;

/// Offset in the filtered retained body, distinct from a matrix row index.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EditSyncBodyRowIndex(usize);
impl EditSyncBodyRowIndex {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }
    pub(super) const fn get(self) -> usize {
        self.0
    }
}

/// A borrowed retained-body view whose dirty offset was checked once at the
/// legacy matrix/filtered-body boundary. It never outlives those immutable rows.
#[derive(Debug)]
pub(crate) struct RetainedEditBody<'slice, 'row> {
    pub(super) rows: &'slice [(usize, &'row super::MatrixRow)],
    pub(super) first_dirty: EditSyncBodyRowIndex,
}
impl<'slice, 'row> RetainedEditBody<'slice, 'row> {
    pub(crate) fn new(
        rows: &'slice [(usize, &'row super::MatrixRow)],
        first_dirty: EditSyncBodyRowIndex,
    ) -> Option<Self> {
        rows.get(first_dirty.get())?;
        Some(Self { rows, first_dirty })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IndependentFrontierRefusal {
    NoPredecessor,
    OutsideDamageBoundary,
    SourceTopology,
    BoxDependency,
    SourceProvenance,
    EndOfBuffer,
    InvalidGeometry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EditSyncRowIndex(usize);

impl EditSyncRowIndex {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }
    pub(crate) const fn get(self) -> usize {
        self.0
    }
    pub(super) fn from_snapshot(index: i64) -> Option<Self> {
        usize::try_from(index).ok().map(Self)
    }
    pub(super) fn membership(self, first: Self) -> SuffixMembership {
        if self >= first {
            SuffixMembership::Keep
        } else {
            SuffixMembership::Discard
        }
    }
}

/// Exclusive upper matrix-row bound for installing the selected text suffix.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EditSyncRowLimit(usize);
impl EditSyncRowLimit {
    pub(crate) const fn new(rows: usize) -> Self {
        Self(rows)
    }
    const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SuffixMembership {
    Keep,
    Discard,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EditSyncWindowY(f32);

impl EditSyncWindowY {
    pub(crate) fn new(y: f32) -> Option<Self> {
        y.is_finite().then_some(Self(y))
    }
    pub(crate) const fn get(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SuffixOrigin {
    source: CharPos0,
    row: EditSyncRowIndex,
    y: EditSyncWindowY,
}

impl SuffixOrigin {
    pub(super) fn from_row(
        index: EditSyncRowIndex,
        row: &GlyphRow,
        damage: EditReplayPositions,
    ) -> Option<Self> {
        let source = i64::try_from(row.start_charpos)
            .ok()?
            .checked_add(damage.delta())?;
        Some(Self {
            source: CharPos0::new(usize::try_from(source).ok()?),
            row: index,
            y: EditSyncWindowY::new(row.pixel_y)?,
        })
    }
    fn stop(self) -> EditSyncStop {
        EditSyncStop {
            stop_charpos: self.source.get(),
            first_unchanged_y: self.y.get(),
        }
    }
}

#[derive(Clone, Debug)]
enum PreparedFrontiers {
    Conservative,
    IndependentNewline { conservative: SuffixOrigin },
}

/// Immutable retained artifacts belong to this exclusive replay attempt.
/// Moving this owner transfers its entire suffix; it cannot choose artifacts
/// from another preparation or materialize both alternatives.
#[derive(Debug)]
pub(crate) struct PreparedEditSuffix {
    union: EditSyncPlan,
    frontiers: PreparedFrontiers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestartInterest {
    NotApplicable,
    Candidate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestartFrontierRefusal {
    NoIndependentFrontier,
    FrontierNotReached,
}

#[derive(Debug)]
pub(crate) enum RestartSelection {
    Selected(SelectedEditSync),
    Refused {
        preparation: PreparedEditSuffix,
        reason: RestartFrontierRefusal,
    },
}

/// The selected plan and the actual reached placement cannot be split or
/// paired with another preparation by callers of the installation API.
#[derive(Debug)]
pub(crate) struct SelectedEditSync {
    plan: EditSyncPlan,
    reached: EditSyncReached,
}

impl From<EditSyncPlan> for PreparedEditSuffix {
    fn from(union: EditSyncPlan) -> Self {
        Self {
            union,
            frontiers: PreparedFrontiers::Conservative,
        }
    }
}

impl PreparedEditSuffix {
    pub(super) fn with_independent_frontier(
        union: EditSyncPlan,
        conservative: SuffixOrigin,
    ) -> Self {
        Self {
            union,
            frontiers: PreparedFrontiers::IndependentNewline { conservative },
        }
    }
    pub(crate) fn face_rows(&self) -> impl Iterator<Item = &GlyphRow> {
        self.union.face_rows()
    }
    pub(crate) fn conservative_stop(&self) -> EditSyncStop {
        match self.frontiers {
            PreparedFrontiers::Conservative => self.union.stop(),
            PreparedFrontiers::IndependentNewline { conservative } => conservative.stop(),
        }
    }
    pub(crate) fn conservative_source_position(&self) -> LayoutCharPos0 {
        LayoutCharPos0::new(self.conservative_stop().stop_charpos as i64)
    }
    pub(crate) fn restart_interest(&self, source: LayoutCharPos0) -> RestartInterest {
        match self.frontiers {
            PreparedFrontiers::Conservative => RestartInterest::NotApplicable,
            PreparedFrontiers::IndependentNewline { .. } => {
                if usize::try_from(source.get()).ok() == Some(self.union.stop_charpos) {
                    RestartInterest::Candidate
                } else {
                    RestartInterest::NotApplicable
                }
            }
        }
    }
    pub(crate) fn select_restart(self, restart: SourceLineRestart<'_>) -> RestartSelection {
        let reason = match self.frontiers {
            PreparedFrontiers::Conservative => RestartFrontierRefusal::NoIndependentFrontier,
            PreparedFrontiers::IndependentNewline { .. } => {
                if let Some(reached) = self.union.stop().reached_at(
                    restart.source_position(),
                    restart.window_y().get(),
                    restart.matrix_row_index().get(),
                ) {
                    return RestartSelection::Selected(SelectedEditSync {
                        plan: self.union,
                        reached,
                    });
                }
                RestartFrontierRefusal::FrontierNotReached
            }
        };
        RestartSelection::Refused {
            preparation: self,
            reason,
        }
    }
    pub(crate) fn select_conservative(mut self, reached: EditSyncReached) -> SelectedEditSync {
        match self.frontiers {
            PreparedFrontiers::Conservative => {}
            PreparedFrontiers::IndependentNewline { conservative } => {
                self.union.select_suffix(conservative)
            }
        }
        SelectedEditSync {
            plan: self.union,
            reached,
        }
    }
    #[cfg(test)]
    pub(super) fn union_for_test(&self) -> &EditSyncPlan {
        &self.union
    }
}

impl EditSyncPlan {
    fn select_suffix(&mut self, origin: SuffixOrigin) {
        let first = origin.row;
        self.rows.select_suffix(first);
        let keep = |row| {
            EditSyncRowIndex::from_snapshot(row)
                .is_some_and(|row| matches!(row.membership(first), SuffixMembership::Keep))
        };
        self.row_snapshots.retain(|row| keep(row.row));
        self.points.retain(|point| keep(point.row));
        if let Some(points) = &mut self.point_rows {
            points.rows.retain(|row| keep(row.row()));
        }
        self.stop_charpos = origin.source.get();
        self.first_unchanged_index = first.get();
        self.first_unchanged_y = origin.y.get();
    }
}

impl SelectedEditSync {
    pub(crate) fn source_position(&self) -> LayoutCharPos0 {
        LayoutCharPos0::new(self.plan.stop_charpos as i64)
    }
    pub(crate) fn install_edit(
        self,
        bottom_y: EditSyncWindowY,
        index_limit: EditSyncRowLimit,
    ) -> EditSyncInstall {
        self.plan
            .install_edit(self.reached, bottom_y.get(), index_limit.get())
    }
    pub(crate) fn install_scroll(
        self,
        bottom_y: EditSyncWindowY,
        index_limit: EditSyncRowLimit,
    ) -> EditSyncInstall {
        self.plan
            .install(self.reached, bottom_y.get(), index_limit.get())
    }
}

static_assertions::const_assert_eq!(
    std::mem::size_of::<EditSyncRowIndex>(),
    std::mem::size_of::<usize>()
);
static_assertions::const_assert_eq!(
    std::mem::size_of::<EditSyncWindowY>(),
    std::mem::size_of::<f32>()
);
static_assertions::const_assert_eq!(
    std::mem::size_of::<Option<EditSyncStop>>(),
    std::mem::size_of::<Option<(usize, f32)>>()
);
static_assertions::const_assert_eq!(
    std::mem::size_of::<Option<EditSyncReached>>(),
    std::mem::size_of::<Option<(usize, f32)>>()
);
static_assertions::assert_impl_all!(PreparedEditSuffix: Send, Sync);

static_assertions::assert_not_impl_any!(PreparedEditSuffix: Clone, Copy);
static_assertions::assert_not_impl_any!(SelectedEditSync: Clone, Copy);
