//! Sealed stopped-tables slot for `CollectorTables` (value-gc B14, G1).
//!
//! Keeping the inner `Rc<Cell<*const ()>>` and the RAII `CollectorTableScope`
//! guard private to this leaf module prevents any code in `tagged::gc` from
//! installing an arbitrary pointer into the slot or `mem::forget`-ing an
//! active scope guard: the sole write path is closure-scoped
//! (`with_entered_on`), which requires a live `&AllStopped` borrow and always
//! clears the slot to null on return and unwind.

use super::mutator_registration::{AllStopped, RendezvousError};
use std::cell::Cell;
use std::rc::Rc;

/// Sealed per-heap cold slot holding an erased pointer to the currently active
/// [`AllStopped`] guard (or null when no stopped-table scope is installed).
pub(super) struct CollectorTablesSlot {
    cell: Rc<Cell<*const ()>>,
}

/// Private RAII guard retaining the active `&'stop AllStopped<'era>` borrow
/// for the duration of its installation in [`CollectorTablesSlot`].
struct CollectorTableScope<'stop, 'era> {
    cell: Rc<Cell<*const ()>>,
    stopped: &'stop AllStopped<'era>,
}

static_assertions::assert_not_impl_any!(
    CollectorTableScope<'static, 'static>: Send,
    Sync,
    Clone,
    Copy
);

impl CollectorTablesSlot {
    pub(super) fn new() -> Self {
        Self {
            cell: Rc::new(Cell::new(std::ptr::null())),
        }
    }

    #[inline]
    pub(super) fn is_installed(&self) -> bool {
        !self.cell.get().is_null()
    }

    /// Read-only raw view for [`super::collector_tables::CollectorTables`].
    #[inline]
    pub(super) fn raw_ptr(&self) -> *const () {
        self.cell.get()
    }

    /// Install `stopped` in the slot selected from `target` for the duration
    /// of `body`, releasing the borrow on `target` before invoking `body` and
    /// clearing the slot on normal return and unwind. Refuses nested
    /// installation with [`RendezvousError::Busy`].
    pub(super) fn with_entered_on<T, R>(
        target: &mut T,
        slot_of: impl FnOnce(&mut T) -> &Self,
        stopped: &AllStopped<'_>,
        body: impl FnOnce(&mut T) -> R,
    ) -> Result<R, RendezvousError> {
        let cell = Rc::clone(&slot_of(target).cell);
        if !cell.get().is_null() {
            return Err(RendezvousError::Busy);
        }
        cell.set(std::ptr::from_ref(stopped).cast());
        let _guard = CollectorTableScope { cell, stopped };
        Ok(body(target))
    }
}

impl Drop for CollectorTableScope<'_, '_> {
    fn drop(&mut self) {
        let installed = self.cell.replace(std::ptr::null());
        debug_assert_eq!(installed, std::ptr::from_ref(self.stopped).cast());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tagged::gc::TaggedHeap;

    #[test]
    fn fresh_collector_tables_slot_is_empty_and_null() {
        let slot = CollectorTablesSlot::new();
        assert!(!slot.is_installed());
        assert!(slot.raw_ptr().is_null());

        let heap = TaggedHeap::new();
        assert!(heap._satb_layout_reserve.collector_tables().is_null());
        let sealed = heap._satb_layout_reserve.collector_tables_slot();
        assert!(!sealed.is_installed());
        assert!(sealed.raw_ptr().is_null());
    }
}
