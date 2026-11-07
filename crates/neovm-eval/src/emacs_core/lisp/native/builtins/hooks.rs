use super::*;
use crate::emacs_core::hook_runtime;
use neovm_value::error::{expect_max_args, expect_min_args};
use neovm_value::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};

use neovm_value::gc_trace::GcTrace;

// ===========================================================================
// Hook system
// ===========================================================================

pub(crate) fn builtin_run_hooks(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    hook_runtime::run_named_hooks(eval, &args)
}

#[doc(hidden)]
pub fn builtin_run_hook_with_args(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    expect_min_args("run-hook-with-args", &args, 1)?;
    hook_runtime::run_named_hook_with_args(eval, &args)
}

#[doc(hidden)]
pub fn builtin_run_hook_with_args_until_success(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_min_args("run-hook-with-args-until-success", &args, 1)?;
    hook_runtime::run_named_hook_with_args_until_success(eval, &args)
}

#[doc(hidden)]
pub fn builtin_run_hook_with_args_until_failure(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("run-hook-with-args-until-failure", &args, 1)?;
    hook_runtime::run_named_hook_with_args_until_failure(eval, &args)
}

#[doc(hidden)]
pub fn builtin_run_hook_wrapped(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    expect_min_args("run-hook-wrapped", &args, 2)?;
    hook_runtime::run_named_hook_wrapped(eval, &args)
}

const WINDOW_CONFIGURATION_TAG: &str = "window-configuration";
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const SAVE_SELECTED_WINDOW_STATE_TAG: &str = "save-selected-window--state";

/// Whether restoring a window configuration also selects its saved frame.
///
/// This is the typed form of GNU `set-window-configuration`'s
/// DONT-SET-FRAME argument.  Naming both states prevents the easy-to-miss
/// inversion caused by passing the Lisp flag deeper into the window code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum SelectedFrameRestoration {
    RestoreSaved,
    KeepSelected,
}

/// Whether restoring a window configuration replaces the frame's live
/// minibuffer window with the saved one.
///
/// This is the typed form of GNU `set-window-configuration`'s
/// DONT-SET-MINIWINDOW argument.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum MinibufferWindowRestoration {
    RestoreSaved,
    KeepCurrent,
}

/// Exhaustive restore policy shared by the Lisp builtin and native unwind
/// actions.  Callers cannot accidentally swap the two same-shaped Lisp flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct WindowConfigurationRestoreOptions {
    pub selected_frame: SelectedFrameRestoration,
    pub minibuffer_window: MinibufferWindowRestoration,
}

/// A validated, GC-traceable window configuration captured from live runtime
/// state.  Native unwind callers cannot manufacture one from an unrelated
/// `Value`; they must cross this constructor boundary.
#[derive(Clone, Copy, Debug)]
#[doc(hidden)]
pub struct SavedWindowConfiguration(Value);

impl SavedWindowConfiguration {
    #[doc(hidden)]
    pub fn capture(eval: &mut super::eval::Context, frame: Value) -> Result<Self, Flow> {
        let configuration =
            crate::emacs_core::eval::lifecycle::window_configuration_capture(eval, vec![frame])?;
        debug_assert!(configuration.is_window_configuration());
        Ok(Self(configuration))
    }

    #[doc(hidden)]
    pub fn trace_value(self) -> Value {
        self.0
    }

    #[doc(hidden)]
    pub fn restore(
        self,
        eval: &mut super::eval::Context,
        options: WindowConfigurationRestoreOptions,
    ) -> EvalResult {
        crate::emacs_core::eval::lifecycle::window_configuration_restore(eval, self.0, options)
    }
}

impl Default for WindowConfigurationRestoreOptions {
    fn default() -> Self {
        Self {
            selected_frame: SelectedFrameRestoration::RestoreSaved,
            minibuffer_window: MinibufferWindowRestoration::RestoreSaved,
        }
    }
}

impl WindowConfigurationRestoreOptions {
    #[doc(hidden)]
    pub fn from_lisp_args(args: &[Value]) -> Self {
        Self {
            selected_frame: if args.get(1).is_some_and(|value| value.is_truthy()) {
                SelectedFrameRestoration::KeepSelected
            } else {
                SelectedFrameRestoration::RestoreSaved
            },
            minibuffer_window: if args.get(2).is_some_and(|value| value.is_truthy()) {
                MinibufferWindowRestoration::KeepCurrent
            } else {
                MinibufferWindowRestoration::RestoreSaved
            },
        }
    }
}

#[doc(hidden)]
pub struct WindowConfigurationSnapshot {
    pub frame_id: neovm_value::display_vocab::FrameId,
    /// Numeric GNU frame epoch at capture. The heap-owned configuration and
    /// its restore are accessed only under the owning Context mutator borrow.
    pub change_stamp: neovm_value::display_vocab::ChangeStamp,
    /// This configuration's window tree.
    ///
    /// A DETACHED copy: `current-window-configuration` clones the frame's tree
    /// and `set-window-configuration` installs it back.  It is a `WindowTree`
    /// rather than a bare `Window` for the same reason `Frame` holds one --
    /// when the tree stores its nodes in an arena, a snapshot has to carry its
    /// own nodes or its child ids refer to windows that are not there.
    pub tree: crate::emacs_core::eval::data::window_state::WindowTree,
    pub selected_window: neovm_edit::emacs_core::display_metrics::WindowId,
    pub current_buffer: Option<neovm_edit::buffer::BufferId>,
    pub minibuffer_window: Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    pub minibuffer_leaf: Option<crate::emacs_core::eval::data::window_state::Window>,
}

impl WindowConfigurationSnapshot {
    #[doc(hidden)]
    pub fn clone_for_restore(&self, buffers: &mut neovm_edit::buffer::BufferManager) -> Self {
        Self {
            frame_id: self.frame_id,
            change_stamp: self.change_stamp,
            tree:
                crate::emacs_core::eval::data::window_markers::clone_window_tree_with_independent_position_markers(
                    buffers, &self.tree,
                ),
            selected_window: self.selected_window,
            current_buffer: self.current_buffer,
            minibuffer_window: self.minibuffer_window,
            minibuffer_leaf: self.minibuffer_leaf.as_ref().map(|window| {
                crate::emacs_core::eval::data::window_markers::clone_window_leaf_with_independent_position_markers(
                    buffers, window,
                )
            }),
        }
    }

    #[doc(hidden)]
    pub fn trace_roots(&self, roots: &mut Vec<Value>) {
        self.tree.trace_roots(roots);
        if let Some(minibuffer) = &self.minibuffer_leaf {
            minibuffer.trace_roots(roots);
        }
    }
}

thread_local! {
    static WINDOW_CONFIGURATION_SNAPSHOTS: HeapRegistrySlot<HashMap<i64, WindowConfigurationSnapshot>> =
        HeapRegistrySlot::new(HashMap::new());
}

pub(super) fn reset_hooks_thread_locals() {
    WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| slot.reset(HashMap::new()));
}

/// Carrier park: forget the installed window-configuration registry handle
/// (a plain store). The World's registry stays alive through its Contexts.
pub(super) fn park_hooks_thread_locals() {
    WINDOW_CONFIGURATION_SNAPSHOTS.with(HeapRegistrySlot::park);
}

pub(crate) type WindowConfigurationRegistryHandle =
    HeapRegistryHandle<HashMap<i64, WindowConfigurationSnapshot>>;

pub(crate) fn current_window_configuration_registry_handle() -> WindowConfigurationRegistryHandle {
    WINDOW_CONFIGURATION_SNAPSHOTS.with(HeapRegistrySlot::current)
}

pub(crate) fn install_window_configuration_registry_handle(
    handle: &WindowConfigurationRegistryHandle,
) {
    WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| slot.install(handle));
}

/// Run `f` over two stored snapshots by serial (window-configuration equality).
///
/// The registry is evaluator data (GC-traced), so the lookup stays here; the
/// core command module supplies the comparison itself.
#[doc(hidden)]
pub fn with_window_configuration_snapshots<R>(
    serial_a: i64,
    serial_b: i64,
    f: impl FnOnce(&WindowConfigurationSnapshot, &WindowConfigurationSnapshot) -> R,
) -> Option<R> {
    WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| {
        let store = slot.borrow();
        match (store.get(&serial_a), store.get(&serial_b)) {
            (Some(a), Some(b)) => Some(f(a, b)),
            _ => None,
        }
    })
}

/// Run `f` over one stored snapshot by serial (window-configuration restore).
#[doc(hidden)]
pub fn with_window_configuration_snapshot<R>(
    serial: i64,
    f: impl FnOnce(&WindowConfigurationSnapshot) -> R,
) -> Option<R> {
    WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| slot.borrow().get(&serial).map(f))
}

/// Store a captured configuration in the bounded side table.
#[doc(hidden)]
pub fn insert_window_configuration_snapshot(serial: i64, snapshot: WindowConfigurationSnapshot) {
    WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| {
        let mut store = slot.borrow_mut();
        store.insert(serial, snapshot);
        if store.len() > 4096
            && let Some(oldest) = store.keys().min().copied()
        {
            store.remove(&oldest);
        }
    });
}

#[doc(hidden)]
pub fn window_configuration_parts_from_value(value: &Value) -> Option<(Value, i64)> {
    // A window-configuration is its own pseudovector type, read via the
    // type-gated accessor (never `as_vector_data`, so it stays opaque to
    // `vectorp`). Slot 0 holds the tag symbol; slots 1/2 the frame and serial.
    let items = value.as_window_configuration_data()?;
    if items.len() != 4 {
        return None;
    }
    match (items[1].kind(), items[2].kind()) {
        (ValueKind::Veclike(VecLikeType::Frame), ValueKind::Fixnum(serial)) => {
            Some((items[1], serial))
        }
        _ => None,
    }
}

#[doc(hidden)]
pub fn window_configuration_frame_from_value(value: &Value) -> Option<Value> {
    window_configuration_parts_from_value(value).map(|(frame, _)| frame)
}

#[doc(hidden)]
pub fn next_window_configuration_serial() -> i64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_WINDOW_CONFIGURATION_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_WINDOW_CONFIGURATION_ID.fetch_add(1, Ordering::Relaxed) as i64
}

#[doc(hidden)]
pub fn make_window_configuration_value(frame: Value, serial: i64, roots: Value) -> Value {
    // GNU stores saved windows inside the opaque window-configuration
    // pseudovector (`src/window.c:Fcurrent_window_configuration`), so the
    // saved Lisp values are traced as part of the object.  Neomacs keeps the
    // Rust window tree in a serial side table; this hidden slot makes the
    // object's GC ownership match GNU's saved-data ownership.
    Value::make_window_configuration(vec![
        Value::symbol(WINDOW_CONFIGURATION_TAG),
        frame,
        Value::fixnum(serial),
        roots,
    ])
}

/// The symbol `features', interned once.
fn features_symbol_id() -> neovm_value::intern::SymId {
    static ID: std::sync::OnceLock<neovm_value::intern::SymId> = std::sync::OnceLock::new();
    *ID.get_or_init(|| neovm_value::intern::intern("features"))
}

#[doc(hidden)]
pub fn builtin_featurep(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    expect_min_args("featurep", &args, 1)?;
    expect_max_args("featurep", &args, 2)?;
    let feature = eval.unwrap_symbol(args[0]);
    let sym_id = feature.as_symbol_id(symbol_mode).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        )
    })?;
    // GNU `Ffeaturep`: `Fmemq (feature, Vfeatures)` on the variable's
    // current value. Rebuilding the feature cache from a copy of the whole
    // list on every call cost ~21K instructions, 4K times per org-journal
    // file open; the walk also signals on an improper `features' as GNU's
    // does. (`features' is not special in GNU, so a lexical `let' of it
    // never reaches here.)
    let features = eval
        .builtin_var_value(features_symbol_id())
        .unwrap_or(Value::NIL);
    if super::cons_list::builtin_memq_values(feature, features, eval.symbols_with_pos_enabled)?
        .is_nil()
    {
        return Ok(Value::NIL);
    }

    let Some(subfeature) = args.get(1) else {
        return Ok(Value::T);
    };
    if subfeature.is_nil() {
        return Ok(Value::T);
    }

    let subfeatures = eval
        .obarray
        .get_property_id(sym_id, neovm_value::intern::intern("subfeatures"))
        .unwrap_or(Value::NIL);
    let items = list_to_vec(&subfeatures).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), subfeatures],
        )
    })?;
    Ok(Value::bool_val(items.iter().any(|item| item == subfeature)))
}

#[cfg(test)]
#[path = "tests/gc_tls_window_configuration_test.rs"]
mod gc_tls_ownership_tests;

/// Debug-only carrier TLS fingerprint for this module's classified records.
#[cfg(debug_assertions)]
pub(crate) fn carrier_tls_fingerprint(h: &mut u64) {
    neovm_value::tls_fingerprint::mix(
        h,
        WINDOW_CONFIGURATION_SNAPSHOTS
            .with(|slot| slot.installed_heap_identity().unwrap_or(0) as u64),
    );
}
