use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

use crate::emacs_core::eval::Context;
use crate::emacs_core::hook_runtime;
use neovm_value::intern::intern;
use neovm_value::value::Value;

type LoadHook = fn();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeapResetMode {
    FreshContext,
    PdumpRestore,
}

#[derive(Clone, Debug)]
struct LoadedDumpStats {
    dump_file_name: PathBuf,
    load_time_secs: f64,
}

#[derive(Default)]
struct PdumpRuntimeState {
    load_hooks: Vec<LoadHook>,
    core_hooks_registered: bool,
    loaded_dump: Option<LoadedDumpStats>,
}

thread_local! {
    static PDUMP_RUNTIME_STATE: RefCell<PdumpRuntimeState> =
        RefCell::new(PdumpRuntimeState::default());
    // The registry and the heap-owned caches its hooks reset belong to this
    // thread. A process-wide guard would leave later threads unregistered.
    static CORE_PDUMP_HOOKS: Once = const { Once::new() };
}

fn hook_identity(hook: LoadHook) -> usize {
    hook as usize
}

pub(crate) fn pdumper_do_now_and_after_load(hook: LoadHook) {
    PDUMP_RUNTIME_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let hook_id = hook_identity(hook);
        if !state
            .load_hooks
            .iter()
            .any(|registered| hook_identity(*registered) == hook_id)
        {
            state.load_hooks.push(hook);
        }
    });
    hook();
}

fn run_registered_load_hooks() {
    let hooks = PDUMP_RUNTIME_STATE.with(|state| state.borrow().load_hooks.clone());
    for hook in hooks {
        hook();
    }
}

fn register_core_load_hooks_into_state() {
    pdumper_do_now_and_after_load(neovm_text::emacs_core::syntax::reset_syntax_thread_locals);
    pdumper_do_now_and_after_load(neovm_text::emacs_core::casetab::reset_casetab_thread_locals);
    pdumper_do_now_and_after_load(neovm_text::emacs_core::regex::reset_regex_thread_locals);
    pdumper_do_now_and_after_load(neovm_value::string_pos_cache::reset_string_pos_cache);
    pdumper_do_now_and_after_load(neovm_text::emacs_core::category::reset_category_thread_locals);
    pdumper_do_now_and_after_load(neovm_value::tagged::value::reset_current_subrs);
    pdumper_do_now_and_after_load(neovm_value::value::reset_string_text_properties);
    pdumper_do_now_and_after_load(neovm_text::emacs_core::ccl::reset_ccl_registry);
    pdumper_do_now_and_after_load(
        crate::emacs_core::eval::data::dynamic_module_registry::reset_dynamic_module_registry,
    );
    // dispnew owns no thread-local state; its reset is empty and
    // stays with the display layer for its own tests. Pdump must not
    // reach into display for a no-op.
    pdumper_do_now_and_after_load(crate::emacs_core::eval::lifecycle::clear_font_cache_state);
    pdumper_do_now_and_after_load(crate::emacs_core::builtins::reset_builtins_thread_locals);
    pdumper_do_now_and_after_load(neovm_text::emacs_core::charset::reset_charset_registry);
    pdumper_do_now_and_after_load(crate::emacs_core::eval::lifecycle::reset_timefns_thread_locals);
    PDUMP_RUNTIME_STATE.with(|state| state.borrow_mut().core_hooks_registered = true);
}

fn register_core_load_hooks() {
    // `CORE_PDUMP_HOOKS` is retained as a vestigial `CarrierIntrinsic` slot
    // (listed in `carrier_tls_class_test.rs`); live per-carrier registration
    // state lives in `PdumpRuntimeState::core_hooks_registered`, which
    // `reset_carrier_tls()` clears alongside `load_hooks`.
    CORE_PDUMP_HOOKS.with(|once| {
        once.call_once(|| {});
    });
    register_core_load_hooks_into_state();
}

pub(crate) fn clear_loaded_dump_stats() {
    PDUMP_RUNTIME_STATE.with(|state| state.borrow_mut().loaded_dump = None);
}

#[derive(Clone, Debug)]
pub(crate) struct SavedLoadedDumpStats(Option<LoadedDumpStats>);

pub(crate) fn snapshot_loaded_dump_stats() -> SavedLoadedDumpStats {
    SavedLoadedDumpStats(PDUMP_RUNTIME_STATE.with(|state| state.borrow().loaded_dump.clone()))
}

pub(crate) fn restore_loaded_dump_stats(saved: SavedLoadedDumpStats) {
    PDUMP_RUNTIME_STATE.with(|state| {
        state.borrow_mut().loaded_dump = saved.0;
    });
}

pub(crate) fn record_loaded_dump(path: &Path, elapsed: Duration) {
    let dump_file_name = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    PDUMP_RUNTIME_STATE.with(|state| {
        state.borrow_mut().loaded_dump = Some(LoadedDumpStats {
            dump_file_name,
            load_time_secs: elapsed.as_secs_f64(),
        });
    });
}

pub(crate) fn pdumper_stats_value() -> Option<Value> {
    let stats = PDUMP_RUNTIME_STATE.with(|state| state.borrow().loaded_dump.clone())?;
    Some(Value::list(vec![
        Value::cons(Value::symbol("dumped-with-pdumper"), Value::T),
        Value::cons(
            Value::symbol("load-time"),
            Value::make_float(stats.load_time_secs),
        ),
        Value::cons(
            Value::symbol("dump-file-name"),
            Value::string(stats.dump_file_name.to_string_lossy().into_owned()),
        ),
    ]))
}

pub(crate) fn reset_runtime_for_new_heap(mode: HeapResetMode) {
    let hooks_already_registered =
        PDUMP_RUNTIME_STATE.with(|state| state.borrow().core_hooks_registered);
    if hooks_already_registered {
        run_registered_load_hooks();
    } else {
        register_core_load_hooks();
    }

    match mode {
        HeapResetMode::FreshContext => {
            // Tests can preconfigure terminal runtime before Context creation.
            crate::emacs_core::eval::lifecycle::reset_terminal_handle();
        }
        HeapResetMode::PdumpRestore => {
            // Terminal thread-local Values (handle AND params) came from
            // the prior TaggedHeap, which has been dropped before this
            // call. Any surviving entry is a stale pointer into freed
            // memory and would UAF on the next GC root walk
            // (collect_terminal_gc_roots traces both handle and params).
            // Unlike FreshContext — which tests may pre-populate — a
            // pdump reload has no carried-over terminal state to keep,
            // so a full reset is correct.
            crate::emacs_core::eval::lifecycle::reset_terminal_thread_locals();
        }
    }

    clear_loaded_dump_stats();
}

pub(crate) fn run_after_pdump_load_hook(eval: &mut Context) {
    let _ = hook_runtime::safe_run_named_hook(eval, intern("after-pdump-load-hook"), &[]);
}

#[cfg(test)]
#[path = "runtime/tests/runtime_test.rs"]
mod tests;

/// Carrier park: reset the per-thread pdump runtime state (including
/// `core_hooks_registered`, so the next `Context` on this OS thread re-arms
/// the core pdump load hooks).
pub(crate) fn reset_carrier_tls() {
    PDUMP_RUNTIME_STATE.with(|state| *state.borrow_mut() = PdumpRuntimeState::default());
}

/// Debug-only carrier TLS fingerprint for this module's classified records.
#[cfg(debug_assertions)]
pub(crate) fn carrier_tls_fingerprint(h: &mut u64) {
    neovm_value::tls_fingerprint::mix(
        h,
        PDUMP_RUNTIME_STATE.with(|slot| {
            let s = slot.borrow();
            ((s.load_hooks.len() as u64) << 2)
                ^ ((s.loaded_dump.is_some() as u64) << 1)
                ^ (s.core_hooks_registered as u64)
        }),
    );
}
