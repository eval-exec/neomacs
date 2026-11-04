//! Eval-side hook publication entry and the typed boot proof (stage-4 E3a).
//!
//! Core's one process-level `install_eval_hooks()` calls
//! [`install_eval_hooks`] here with the single canonical [`EvalHookSet`].
//! This entry publishes every cold PC2 slot the evaluator needs, in one
//! FIXED order, and mints [`EvalHooksInstalled`] only after the last
//! publication succeeds. Every eval `Context` constructor (`new_inner`,
//! `from_dump`, and the bootstrap constructors) takes the proof by value, so
//! the GC-root-enumerating slots provably exist before the first possible
//! allocation (peer E3a, refining E3).
//!
//! The proof is a `Copy` zero-sized type with a private constructor.
//! Installation is process-permanent (each table's `OnceLock`), so an
//! existing `Context` implies the proof; eval-internal worker/owner
//! constructors obtain it from the parent through
//! [`Context::hooks_installed`] and never re-install.
//!
//! A conflicting second set is the existing typed refusal
//! (`HookInstallError::DifferentTable`), never ignored. The same set
//! re-publishes as `AlreadyInstalled` and still yields the proof.

use super::*;
use neovm_value::HookInstallError;
use std::sync::OnceLock;

/// The one core-owned set of eval cold tables.
///
/// Field order is the publication order and is part of the contract: the
/// lifecycle descriptor (which carries the GC-root enumerators and the
/// pdump-reconstruct resets) publishes first.
#[derive(Clone, Copy)]
pub struct EvalHookSet {
    lifecycle: &'static lifecycle::LifecycleHooks,
    process: &'static process_hooks::ProcessHooks,
    input: &'static input_hooks::InputHooks,
    call_interactively: &'static call_interactively::CallInteractivelyHooks,
    case_tables: &'static case_tables::CaseTableHooks,
    buffer_kill: &'static buffer_kill::BufferKillHooks,
    kmacro: &'static kmacro_hooks::KmacroHooks,
    display_query: &'static display_query_hooks::DisplayQueryHooks,
    perf_enabled: bool,
    perf: &'static perf::PerfHooks,
}

impl EvalHookSet {
    /// Build the one canonical set core owns.
    #[allow(clippy::too_many_arguments)] // one cold descriptor, one install site
    pub const fn new(
        lifecycle: &'static lifecycle::LifecycleHooks,
        process: &'static process_hooks::ProcessHooks,
        input: &'static input_hooks::InputHooks,
        call_interactively: &'static call_interactively::CallInteractivelyHooks,
        case_tables: &'static case_tables::CaseTableHooks,
        buffer_kill: &'static buffer_kill::BufferKillHooks,
        kmacro: &'static kmacro_hooks::KmacroHooks,
        display_query: &'static display_query_hooks::DisplayQueryHooks,
        perf_enabled: bool,
        perf: &'static perf::PerfHooks,
    ) -> Self {
        Self {
            lifecycle,
            process,
            input,
            call_interactively,
            case_tables,
            buffer_kill,
            kmacro,
            display_query,
            perf_enabled,
            perf,
        }
    }
}

/// Proof that every eval cold table was published in this process.
///
/// `Copy` zero-sized type with a private constructor; installation is
/// process-permanent, and every constructor requires the value.
#[derive(Clone, Copy)]
pub struct EvalHooksInstalled(());

impl EvalHooksInstalled {
    /// Mint the proof. Reachable only from this module: the publication
    /// entry (after the last slot) and `Context::hooks_installed` (an
    /// existing Context implies permanent installation).
    fn mint() -> Self {
        Self(())
    }
}

/// Set once the publication entry has returned the proof at least once.
/// Not TLS and not stored in `Context`; the proof carries no state.
static HOOKS_INSTALLED: OnceLock<()> = OnceLock::new();

/// Publish every eval cold table in the fixed order and mint the boot proof.
///
/// Core's `install_eval_hooks` is the exact caller. The proof is minted only
/// after the last publication succeeds; a conflicting set returns the typed
/// refusal before any proof exists. Re-running with the same set publishes
/// each slot as `AlreadyInstalled` and still returns the proof.
#[doc(hidden)]
pub fn install_eval_hooks(
    set: &'static EvalHookSet,
) -> Result<EvalHooksInstalled, HookInstallError> {
    // 1. Lifecycle: the GC-root-enumerating dynamic-module slot, the
    //    pdump-reconstruct resets and the boot entries publish first.
    lifecycle::install_lifecycle_hooks(set.lifecycle)?;
    // 2. Process and input wait/OS-signal slots.
    process_hooks::install_process_hooks(set.process)?;
    input_hooks::install_input_hooks(set.input)?;
    // 3. Command-layer cold tables.
    call_interactively::install_call_interactively_hooks(set.call_interactively)?;
    case_tables::install_case_table_hooks(set.case_tables)?;
    buffer_kill::install_buffer_kill_hooks(set.buffer_kill)?;
    kmacro_hooks::install_kmacro_hooks(set.kmacro)?;
    // 4. Display policy queries.
    display_query_hooks::install_display_query_hooks(set.display_query)?;
    // 5. Hot-path timing pair (also carries its enable answer).
    perf::install_perf_hooks(set.perf_enabled, set.perf)?;
    // The proof exists only now: every GC-root-enumerating and
    // pdump-reconstruct slot above is published.
    HOOKS_INSTALLED.get_or_init(|| ());
    Ok(EvalHooksInstalled::mint())
}

impl Context {
    /// The boot proof for an eval-internal worker/owner constructor.
    ///
    /// An existing `Context` can only have been built with the proof, and
    /// installation is process-permanent, so the value is always available.
    /// These constructors never re-install.
    pub fn hooks_installed(&self) -> EvalHooksInstalled {
        assert!(
            HOOKS_INSTALLED.get().is_some(),
            "an existing Context implies a published eval hook set"
        );
        EvalHooksInstalled::mint()
    }
}
