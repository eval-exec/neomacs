//! Case-table hook table.
//!
//! The production adapter lives in `neovm-eval::case_table_adapter`. This
//! module publishes that one table and re-exports the adapter so existing
//! core callers keep their paths. There is no second copy of the bodies.

pub(crate) use neovm_eval::emacs_core::eval::case_table_adapter::{
    builtin_current_case_table, builtin_set_case_table, builtin_set_standard_case_table,
    builtin_standard_case_table, case_table_override_for_current_buffer, current_case_canon_table,
    current_case_upcase_table, standard_case_table_for_admission,
    sync_current_buffer_case_table_state,
};

static CANONICAL_CASE_TABLE_HOOKS: crate::emacs_core::eval::case_tables::CaseTableHooks =
    crate::emacs_core::eval::case_tables::CaseTableHooks::new(
        current_case_canon_table,
        current_case_upcase_table,
        sync_current_buffer_case_table_state,
        standard_case_table_for_admission,
        case_table_override_for_current_buffer,
    );

/// The one canonical case-table descriptor.
pub(crate) fn canonical_case_table_hooks()
-> &'static crate::emacs_core::eval::case_tables::CaseTableHooks {
    &CANONICAL_CASE_TABLE_HOOKS
}
