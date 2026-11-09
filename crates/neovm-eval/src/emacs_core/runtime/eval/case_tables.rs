//! Cold case-table read/sync table (stage-4 W4/E12).
//!
//! The case-table authority stays in the core command layer
//! (`commands/casetab_entries`): it binds the raw obarray legacy write, the
//! green standard-table setter, the cached standard table and the tagged-heap
//! identity (stage-3b R0/T5). The evaluator's per-operation search and string
//! paths read the current canon/upcase tables and sync the current buffer's
//! table through this one cold PC2 table, installed by core at
//! `Context::new_inner` and `Context::from_dump`.
//!
//! Frequency (E4 proof — every slot is read once per case-sensitive operation
//! at a hoisted call site, never inside a per-char matcher loop):
//! - `builtin_string_match_slice` (search.rs) reads `canon` before compiling
//!   the matcher and before the search loop;
//! - `string_match_fast` (search.rs) reads `canon` once for its whole fast
//!   path;
//! - `builtin_posix_string_match` (search.rs) reads `canon` once before the
//!   regexp compile;
//! - `builtin_compare_buffer_substrings` (misc_eval.rs) and
//!   `CompareStringsCase::for_current_buffer` (fns/mod.rs) read once per call;
//! - `sync_current_buffer_runtime_state` (gc_pacing.rs) syncs once per buffer
//!   switch;
//! - `fix_standard_case_table_cell` (admission.rs) runs only at admission.

use super::*;
use neovm_value::{HookInstallError, HookInstallStatus};
use std::sync::OnceLock;

/// Read the current buffer's canonical (or upcase) derived case table.
pub type CaseTableRead = fn(&mut Context) -> Result<Value, Flow>;
/// Re-derive/validate the current buffer's case table after a buffer switch.
pub type CaseTableSync = fn(&mut Context) -> Result<(), Flow>;

/// The four cold bodies eval calls per case-sensitive operation.
///
/// `standard` is the standard-table install that admission performs. It is a
/// separate slot because admission needs the standard table object, not the
/// current buffer's derived tables.
#[derive(Clone, Copy)]
pub struct CaseTableHooks {
    canon: CaseTableRead,
    upcase: CaseTableRead,
    sync: CaseTableSync,
    standard: CaseTableRead,
    override_for_current_buffer:
        fn(&mut Context) -> Result<neovm_text::emacs_core::casetab::CaseTableOverride, Flow>,
}

impl CaseTableHooks {
    /// Build the one canonical descriptor core publishes.
    pub const fn new(
        canon: CaseTableRead,
        upcase: CaseTableRead,
        sync: CaseTableSync,
        standard: CaseTableRead,
        override_for_current_buffer: fn(
            &mut Context,
        ) -> Result<
            neovm_text::emacs_core::casetab::CaseTableOverride,
            Flow,
        >,
    ) -> Self {
        Self {
            canon,
            upcase,
            sync,
            standard,
            override_for_current_buffer,
        }
    }

    /// GNU's current-buffer canonical case table.
    pub fn canon(&self) -> CaseTableRead {
        self.canon
    }

    /// GNU's current-buffer upcase table.
    pub fn upcase(&self) -> CaseTableRead {
        self.upcase
    }

    /// Re-derive the current buffer's case table after a buffer switch.
    pub fn sync(&self) -> CaseTableSync {
        self.sync
    }

    /// Install/return the standard case table (admission).
    pub fn standard(&self) -> CaseTableRead {
        self.standard
    }

    /// The current buffer's case-table override, for casefiddle/string ops.
    pub fn override_for_current_buffer(
        &self,
    ) -> fn(&mut Context) -> Result<neovm_text::emacs_core::casetab::CaseTableOverride, Flow> {
        self.override_for_current_buffer
    }
}

static CASE_TABLE_HOOKS: OnceLock<CaseTableHooks> = OnceLock::new();
static CASE_TABLE_HOOKS_IDENTITY: OnceLock<&'static CaseTableHooks> = OnceLock::new();

/// Publish the core command layer's case-table bodies.
///
/// Idempotent for the same descriptor; a different table is a typed refusal
/// (`HookInstallError::DifferentTable`), never a silent overwrite. Called only
/// by the one eval `install_eval_hooks` entry (E3a); core no longer calls this
/// per-table installer (peer follow-up F2).
#[must_use = "handle a conflicting case-table publication"]
#[cold]
#[inline(never)]
pub(crate) fn install_case_table_hooks(
    hooks: &'static CaseTableHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    neovm_value::publish_hook_table(&CASE_TABLE_HOOKS, &CASE_TABLE_HOOKS_IDENTITY, hooks)
}

/// The installed table, if core has run one of the two pinned install seams.
pub fn case_table_hooks() -> Option<&'static CaseTableHooks> {
    CASE_TABLE_HOOKS.get()
}

/// Typed refusal for the unreachable uninstalled path: reaching it means the
/// construction seam itself is broken, so never default to the standard table.
fn hooks_or_refusal() -> Result<&'static CaseTableHooks, Flow> {
    case_table_hooks().ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("case-table hooks are not installed")],
        )
    })
}

/// Read GNU's current-buffer canonical case table through the cold table.
pub fn current_case_canon_table(ctx: &mut Context) -> Result<Value, Flow> {
    (hooks_or_refusal()?.canon())(ctx)
}

/// Read GNU's current-buffer upcase table through the cold table.
pub fn current_case_upcase_table(ctx: &mut Context) -> Result<Value, Flow> {
    (hooks_or_refusal()?.upcase())(ctx)
}

/// Sync the current buffer's case-table state after a buffer switch.
pub fn sync_current_buffer_case_table_state(ctx: &mut Context) -> Result<(), Flow> {
    (hooks_or_refusal()?.sync())(ctx)
}

/// Install/return the standard case table for admission.
pub fn standard_case_table(ctx: &mut Context) -> Result<Value, Flow> {
    (hooks_or_refusal()?.standard())(ctx)
}

/// The current buffer's case-table override (GNU's per-buffer case tables).
pub fn case_table_override_for_current_buffer(
    ctx: &mut Context,
) -> Result<neovm_text::emacs_core::casetab::CaseTableOverride, Flow> {
    (hooks_or_refusal()?.override_for_current_buffer())(ctx)
}
