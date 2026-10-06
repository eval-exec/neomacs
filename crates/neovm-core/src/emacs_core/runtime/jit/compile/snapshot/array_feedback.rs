//! Opt/Range-only array feedback attached to an unchanged main snapshot.
//!
//! Threading: each snapshot owns copied scalar masks and main's existing source
//! Arc. The selected carrier is Send; an owning frontend captures physical-kind
//! metadata before transfer. Workers may publish this owned carrier directly,
//! without consulting worker knobs, dereferencing Lisp objects, or borrowing a
//! mutator's source table. Main's snapshot and scope layouts stay unchanged.

use super::super::*;
use super::{FeedbackSnapshot, NumericFeedbackScope};
use crate::emacs_core::jit::feedback::arrays::ArrayKindMask;

/// Additional owned compiler masks, keeping the complete main snapshot intact.
#[derive(Debug, Default)]
pub(crate) struct SelectedFeedbackSnapshot {
    pub(crate) main: FeedbackSnapshot,
    pub(crate) array: Vec<ArrayKindMask>,
}

static_assertions::assert_impl_all!(SelectedFeedbackSnapshot: Send);

impl SelectedFeedbackSnapshot {
    /// The frontend calls this only for Opt plus Range. Normal T1/Aref profile
    /// selection controls source-table creation; empty masks still belong to
    /// this source, including Tier2-off and Generic-retreat compilations.
    pub(crate) fn take(f: &ByteCodeFunction) -> Self {
        debug_assert!(super::super::array_snapshot::selected());
        let mut main = FeedbackSnapshot::take(f);
        let rt = f.jit_runtime();
        let ops = f.executable_ops();
        let array_profile = jit_tier2().on && ops.iter().any(|op| *op == Op::Aref);
        let array = if array_profile {
            rt.array_sites_for(ops);
            if rt.reopt_level() < crate::emacs_core::jit::ReoptLevel::Generic {
                rt.array_kind_snapshot(ops.len())
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        if array_profile && main.call_source.is_none() {
            // Feedback=off still owns every source-held site later baked by
            // the selected T1 observer. Main call-target policy stays intact.
            main.call_source = Some(rt.share_state());
        }
        Self { main, array }
    }

    /// Publish already-owned data: worker selectors cannot affect its contents.
    /// Always replace even an empty selected array vector so a nested source
    /// cannot inherit outer hints. Drop restores main first, then array masks,
    /// matching the original integrated source24 scope's restoration order.
    pub(crate) fn publish(self) -> SelectedNumericFeedbackScope {
        SelectedNumericFeedbackScope {
            _main: self.main.publish(),
            _array: super::super::array_snapshot::ArrayFeedbackScope::enter(self.array),
        }
    }
}

/// Compiler-only owning guard; field declaration order controls restoration.
pub(crate) struct SelectedNumericFeedbackScope {
    _main: NumericFeedbackScope,
    _array: super::super::array_snapshot::ArrayFeedbackScope,
}

/// Frontend scope choice; OFF/Legacy owns only main's original scope and never
/// enters array TLS. No runtime cache/layout or extra source owner is added.
pub(crate) enum FrontFeedbackScope {
    Main {
        _scope: NumericFeedbackScope,
    },
    Array {
        _scope: SelectedNumericFeedbackScope,
    },
}

/// The sole frontend selector. Old take/publish functions stay literal main;
/// workers receiving a selected snapshot invoke its owned publish directly.
pub(crate) fn publish_numeric_feedback_with_arrays(f: &ByteCodeFunction) -> FrontFeedbackScope {
    if !super::super::array_snapshot::selected() {
        return FrontFeedbackScope::Main {
            _scope: super::publish_numeric_feedback(f),
        };
    }
    FrontFeedbackScope::Array {
        _scope: SelectedFeedbackSnapshot::take(f).publish(),
    }
}
