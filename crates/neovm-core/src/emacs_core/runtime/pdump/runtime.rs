use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

use crate::emacs_core::eval::Context;
use crate::emacs_core::hook_runtime;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::Value;

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
    loaded_dump: Option<LoadedDumpStats>,
}

thread_local! {
    static PDUMP_RUNTIME_STATE: RefCell<PdumpRuntimeState> =
        RefCell::new(PdumpRuntimeState::default());
}

static CORE_PDUMP_HOOKS: Once = Once::new();

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

fn register_core_load_hooks() {
    CORE_PDUMP_HOOKS.call_once(|| {
        pdumper_do_now_and_after_load(crate::emacs_core::syntax::reset_syntax_thread_locals);
        pdumper_do_now_and_after_load(crate::emacs_core::casetab::reset_casetab_thread_locals);
        pdumper_do_now_and_after_load(crate::emacs_core::category::reset_category_thread_locals);
        pdumper_do_now_and_after_load(crate::tagged::value::reset_current_subrs);
        pdumper_do_now_and_after_load(crate::emacs_core::value::reset_string_text_properties);
        pdumper_do_now_and_after_load(crate::emacs_core::ccl::reset_ccl_registry);
        pdumper_do_now_and_after_load(
            crate::emacs_core::dispnew::pure::reset_dispnew_thread_locals,
        );
        pdumper_do_now_and_after_load(crate::emacs_core::xfaces::clear_font_cache_state);
        pdumper_do_now_and_after_load(crate::emacs_core::builtins::reset_builtins_thread_locals);
        pdumper_do_now_and_after_load(crate::emacs_core::charset::reset_charset_registry);
        pdumper_do_now_and_after_load(crate::emacs_core::timefns::reset_timefns_thread_locals);
    });
}

pub(crate) fn clear_loaded_dump_stats() {
    PDUMP_RUNTIME_STATE.with(|state| state.borrow_mut().loaded_dump = None);
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
    let hooks_already_registered = CORE_PDUMP_HOOKS.is_completed();
    register_core_load_hooks();
    if hooks_already_registered {
        run_registered_load_hooks();
    }

    match mode {
        HeapResetMode::FreshContext => {
            // Tests can preconfigure terminal runtime before Context creation.
            crate::emacs_core::terminal::pure::reset_terminal_handle();
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
            crate::emacs_core::terminal::pure::reset_terminal_thread_locals();
        }
    }

    clear_loaded_dump_stats();
}

pub(crate) fn run_after_pdump_load_hook(eval: &mut Context) {
    let _ = hook_runtime::safe_run_named_hook(eval, intern("after-pdump-load-hook"), &[]);
}
