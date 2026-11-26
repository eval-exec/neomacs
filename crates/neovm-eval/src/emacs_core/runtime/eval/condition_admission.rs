//! Explicit admission of published condition snapshots on the serial context.
//!
//! Startup does not call [`Context::admit_condition_snapshots`]. The field
//! stays [`ConditionSnapshotAdmission::Dormant`] until a test does.

use super::{ConditionSnapshotAdmission, ConditionSnapshotAdmitError, Context};

impl Context {
    /// Mint one publisher for this context's obarray.
    ///
    /// A second call returns [`ConditionSnapshotAdmitError::AlreadyActive`]
    /// and does not mint another id. Drop unregisters the id.
    pub(crate) fn admit_condition_snapshots(&mut self) -> Result<(), ConditionSnapshotAdmitError> {
        self.condition_snapshots
            .admit(&mut self.tagged_heap, &self.obarray)
    }
}

#[cfg(test)]
#[path = "tests/condition_snapshot_admission_test.rs"]
mod tests;
