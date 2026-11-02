//! Lower finalizer hook for [`SqliteObj`].
//!
//! `SqliteObj` moves to neovm-value while the sqlite thread-local registries
//! stay in neovm-core. The sqlite module installs its one finalizer through
//! this typed table beside the other one-time hooks and again at the top of its
//! handle-construction path; until then `Drop` is the signed typed no-op. No
//! registry logic crosses the boundary, only this function pointer.

use super::SqliteObj;
use std::sync::OnceLock;

/// The sqlite module's object finalizer.
#[derive(Clone, Copy)]
pub(crate) struct SqliteObjFinalizer {
    pub finalize: fn(&SqliteObj),
}

static SQLITE_OBJ_FINALIZER: OnceLock<SqliteObjFinalizer> = OnceLock::new();

/// Install the sqlite finalizer. Idempotent like the other one-time hooks.
pub(crate) fn install_sqlite_obj_finalizer(finalizer: SqliteObjFinalizer) {
    let _ = SQLITE_OBJ_FINALIZER.set(finalizer);
}

/// Whether the finalizer hook has been installed (test-only observation).
#[cfg(test)]
pub(crate) fn sqlite_obj_finalizer_installed() -> bool {
    SQLITE_OBJ_FINALIZER.get().is_some()
}

/// Run the finalizer with an explicitly supplied table so the signed typed
/// no-op and the delegation branch are both testable without global state.
pub(crate) fn finalize_with(obj: &SqliteObj, table: Option<&SqliteObjFinalizer>) {
    if let Some(table) = table {
        (table.finalize)(obj);
    }
}

impl Drop for SqliteObj {
    fn drop(&mut self) {
        finalize_with(self, SQLITE_OBJ_FINALIZER.get());
    }
}
