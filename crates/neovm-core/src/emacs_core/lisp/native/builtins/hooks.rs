use super::*;
use crate::tagged::SymbolMode;
mod window_configuration_geometry;
mod window_configuration_redisplay;
use crate::buffer::LispCharPos1;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_max_args, expect_min_args};
use crate::emacs_core::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};
use crate::emacs_core::hook_runtime;

use crate::gc_trace::GcTrace;

// ===========================================================================
// Hook system
// ===========================================================================

pub(crate) fn builtin_run_hooks(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    hook_runtime::run_named_hooks(eval, &args)
}

pub(crate) fn builtin_run_hook_with_args(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_min_args("run-hook-with-args", &args, 1)?;
    hook_runtime::run_named_hook_with_args(eval, &args)
}

pub(crate) fn builtin_run_hook_with_args_until_success(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_min_args("run-hook-with-args-until-success", &args, 1)?;
    hook_runtime::run_named_hook_with_args_until_success(eval, &args)
}

pub(crate) fn builtin_run_hook_with_args_until_failure(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("run-hook-with-args-until-failure", &args, 1)?;
    hook_runtime::run_named_hook_with_args_until_failure(eval, &args)
}

pub(crate) fn builtin_run_hook_wrapped(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
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
pub(crate) enum SelectedFrameRestoration {
    RestoreSaved,
    KeepSelected,
}

/// Whether restoring a window configuration replaces the frame's live
/// minibuffer window with the saved one.
///
/// This is the typed form of GNU `set-window-configuration`'s
/// DONT-SET-MINIWINDOW argument.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MinibufferWindowRestoration {
    RestoreSaved,
    KeepCurrent,
}

/// Exhaustive restore policy shared by the Lisp builtin and native unwind
/// actions.  Callers cannot accidentally swap the two same-shaped Lisp flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WindowConfigurationRestoreOptions {
    pub(crate) selected_frame: SelectedFrameRestoration,
    pub(crate) minibuffer_window: MinibufferWindowRestoration,
}

/// A validated, GC-traceable window configuration captured from live runtime
/// state.  Native unwind callers cannot manufacture one from an unrelated
/// `Value`; they must cross this constructor boundary.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SavedWindowConfiguration(Value);

impl SavedWindowConfiguration {
    pub(crate) fn capture(eval: &mut super::eval::Context, frame: Value) -> Result<Self, Flow> {
        let configuration = builtin_current_window_configuration(eval, vec![frame])?;
        debug_assert!(configuration.is_window_configuration());
        Ok(Self(configuration))
    }

    pub(crate) fn trace_value(self) -> Value {
        self.0
    }

    pub(crate) fn restore(
        self,
        eval: &mut super::eval::Context,
        options: WindowConfigurationRestoreOptions,
    ) -> EvalResult {
        set_window_configuration_with_options(eval, self.0, options)
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
    fn from_lisp_args(args: &[Value]) -> Self {
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

pub(crate) struct WindowConfigurationSnapshot {
    frame_id: neovm_value::display_vocab::FrameId,
    /// Numeric GNU frame epoch at capture. The heap-owned configuration and
    /// its restore are accessed only under the owning Context mutator borrow.
    change_stamp: neovm_value::display_vocab::ChangeStamp,
    /// This configuration's window tree.
    ///
    /// A DETACHED copy: `current-window-configuration` clones the frame's tree
    /// and `set-window-configuration` installs it back.  It is a `WindowTree`
    /// rather than a bare `Window` for the same reason `Frame` holds one --
    /// when the tree stores its nodes in an arena, a snapshot has to carry its
    /// own nodes or its child ids refer to windows that are not there.
    tree: crate::window::WindowTree,
    selected_window: neovm_edit::emacs_core::display_metrics::WindowId,
    current_buffer: Option<crate::buffer::BufferId>,
    minibuffer_window: Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    minibuffer_leaf: Option<crate::window::Window>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DisconnectedWindowPoint {
    PreserveSelectedBuffer,
    PreserveLastSelectedWindow(neovm_edit::emacs_core::display_metrics::WindowId),
    CommitWindowPoint(LispCharPos1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OutgoingWindowBuffer {
    window_id: neovm_edit::emacs_core::display_metrics::WindowId,
    buffer_id: crate::buffer::BufferId,
    window_start: LispCharPos1,
    window_point: LispCharPos1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReusedWindowHistoryTransition {
    PreserveLiveHistory,
    RecordOutgoingBuffer {
        outgoing: OutgoingWindowBuffer,
        incoming_buffer_id: crate::buffer::BufferId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SavedWindowBufferRestoration {
    RestoreSaved,
    KeepReusedWindow {
        buffer_id: crate::buffer::BufferId,
        point: LispCharPos1,
    },
    FindSubstitute,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CurrentWindowBuffer {
    buffer_id: crate::buffer::BufferId,
    point: LispCharPos1,
}

fn apply_saved_window_buffer_restoration(
    tree: &mut crate::window::WindowTree,
    current_buffers: &HashMap<
        neovm_edit::emacs_core::display_metrics::WindowId,
        CurrentWindowBuffer,
    >,
    buffers: &mut crate::buffer::BufferManager,
) {
    tree.for_each_leaf_mut(|window| {
        apply_saved_leaf_buffer_restoration(window, current_buffers, buffers);
    });
}

fn apply_saved_leaf_buffer_restoration(
    window: &mut crate::window::Window,
    current_buffers: &HashMap<
        neovm_edit::emacs_core::display_metrics::WindowId,
        CurrentWindowBuffer,
    >,
    buffers: &mut crate::buffer::BufferManager,
) {
    match window {
        crate::window::Window::Leaf {
            id,
            buffer_id,
            window_start,
            position_markers,
            point,
            old_point,
            ..
        } => {
            let restoration = if buffers.get(*buffer_id).is_some() {
                SavedWindowBufferRestoration::RestoreSaved
            } else if let Some(current) = current_buffers.get(id) {
                SavedWindowBufferRestoration::KeepReusedWindow {
                    buffer_id: current.buffer_id,
                    point: current.point,
                }
            } else {
                SavedWindowBufferRestoration::FindSubstitute
            };

            match restoration {
                SavedWindowBufferRestoration::RestoreSaved => {}
                SavedWindowBufferRestoration::KeepReusedWindow {
                    buffer_id: current_buffer_id,
                    point: current_point,
                } => {
                    *buffer_id = current_buffer_id;
                    *window_start = LispCharPos1::ONE;
                    *point = current_point;
                    *old_point = current_point;
                    *position_markers = crate::window::WindowPositionMarkerState::Detached;
                    crate::window::window_markers::attach_window_position_markers(buffers, window);
                }
                SavedWindowBufferRestoration::FindSubstitute => {}
            }
        }
        crate::window::Window::Internal { .. } => {}
    }
}

fn prepare_saved_window_buffer_restoration(
    eval: &mut super::eval::Context,
    snapshot: &mut WindowConfigurationSnapshot,
) {
    let globally_selected_window = eval
        .frames
        .selected_frame()
        .map(|frame| frame.selected_window);
    let current_buffers = eval
        .frames
        .get(snapshot.frame_id)
        .map(|frame| {
            frame
                .window_list()
                .into_iter()
                .filter_map(|window_id| {
                    let crate::window::Window::Leaf {
                        buffer_id, point, ..
                    } = frame.find_window(window_id)?
                    else {
                        return None;
                    };
                    let point = if globally_selected_window == Some(window_id) {
                        eval.edit
                            .buffers
                            .get(*buffer_id)
                            .map(|buffer| {
                                LispCharPos1::from_one_based_usize(
                                    buffer.point_char_pos().get().saturating_add(1),
                                )
                            })
                            .unwrap_or(*point)
                    } else {
                        *point
                    };
                    Some((
                        window_id,
                        CurrentWindowBuffer {
                            buffer_id: *buffer_id,
                            point,
                        },
                    ))
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    apply_saved_window_buffer_restoration(
        &mut snapshot.tree,
        &current_buffers,
        &mut eval.edit.buffers,
    );
}

fn collect_leaf_buffer_ids(
    tree: &crate::window::WindowTree,
    buffers: &mut HashMap<
        neovm_edit::emacs_core::display_metrics::WindowId,
        crate::buffer::BufferId,
    >,
) {
    for id in tree.leaf_ids() {
        if let Some(crate::window::Window::Leaf { buffer_id, .. }) = tree.find(id) {
            buffers.insert(id, *buffer_id);
        }
    }
}

fn merge_live_window_histories(
    tree: &mut crate::window::WindowTree,
    live_histories: &HashMap<
        neovm_edit::emacs_core::display_metrics::WindowId,
        neovm_value::display_vocab::WindowHistoryState,
    >,
) {
    tree.for_each_leaf_mut(|window| {
        if let crate::window::Window::Leaf { id, history, .. } = window
            && let Some(live_history) = live_histories.get(id)
        {
            *history = live_history.clone();
        }
    });
}

fn prepare_reused_window_histories(
    eval: &mut super::eval::Context,
    snapshot: &mut WindowConfigurationSnapshot,
) -> Result<(), Flow> {
    let mut saved_buffers = HashMap::new();
    collect_leaf_buffer_ids(&snapshot.tree, &mut saved_buffers);

    let globally_selected_window = eval
        .frames
        .selected_frame()
        .map(|frame| frame.selected_window);
    let transitions = eval
        .frames
        .get(snapshot.frame_id)
        .map(|frame| {
            frame
                .window_list()
                .into_iter()
                .filter_map(|window_id| {
                    let saved_buffer_id = saved_buffers.get(&window_id).copied()?;
                    let crate::window::Window::Leaf {
                        buffer_id,
                        window_start,
                        point,
                        ..
                    } = frame.find_window(window_id)?
                    else {
                        return None;
                    };
                    let window_point = if globally_selected_window == Some(window_id) {
                        eval.edit
                            .buffers
                            .get(*buffer_id)
                            .map(|buffer| {
                                LispCharPos1::from_one_based_usize(
                                    buffer.point_char_pos().get().saturating_add(1),
                                )
                            })
                            .unwrap_or(*point)
                    } else {
                        *point
                    };
                    let outgoing = OutgoingWindowBuffer {
                        window_id,
                        buffer_id: *buffer_id,
                        window_start: *window_start,
                        window_point,
                    };
                    if outgoing.buffer_id != saved_buffer_id
                        && eval.edit.buffers.get(saved_buffer_id).is_some()
                    {
                        Some(ReusedWindowHistoryTransition::RecordOutgoingBuffer {
                            outgoing,
                            incoming_buffer_id: saved_buffer_id,
                        })
                    } else {
                        Some(ReusedWindowHistoryTransition::PreserveLiveHistory)
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for transition in transitions {
        match transition {
            ReusedWindowHistoryTransition::PreserveLiveHistory => {}
            ReusedWindowHistoryTransition::RecordOutgoingBuffer {
                outgoing,
                incoming_buffer_id,
            } => {
                let run_buffer_list_hook = {
                    let (frames, buffers, minibuffers) =
                        (&mut eval.frames, &eval.edit.buffers, &eval.minibuffers);
                    super::window_cmds::record_window_buffer_change_history_in_state(
                        frames,
                        minibuffers,
                        buffers,
                        snapshot.frame_id,
                        outgoing.window_id,
                        crate::emacs_core::eval::data::window_state::WindowBufferHistoryChange {
                            outgoing_buffer_id: outgoing.buffer_id,
                            incoming_buffer_id,
                            outgoing_window_start: outgoing.window_start,
                            outgoing_window_point: outgoing.window_point,
                        },
                    )?
                };
                if run_buffer_list_hook {
                    super::super::buffer::run_buffer_list_update_hook(eval)?;
                }
            }
        }
    }

    let live_histories = eval
        .frames
        .get(snapshot.frame_id)
        .map(|frame| {
            frame
                .window_list()
                .into_iter()
                .filter_map(|window_id| {
                    frame
                        .find_window(window_id)
                        .and_then(crate::window::Window::history)
                        .cloned()
                        .map(|history| (window_id, history))
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    merge_live_window_histories(&mut snapshot.tree, &live_histories);
    Ok(())
}

fn live_window_displays_buffer(
    frames: &crate::window::FrameManager,
    window_id: neovm_edit::emacs_core::display_metrics::WindowId,
    buffer_id: crate::buffer::BufferId,
) -> bool {
    frames
        .lookup_window(window_id)
        .and_then(crate::window::Window::buffer_id)
        == Some(buffer_id)
}

fn disconnected_window_point(
    eval: &super::eval::Context,
    outgoing: OutgoingWindowBuffer,
) -> DisconnectedWindowPoint {
    let selected_buffer_id = eval
        .frames
        .selected_frame()
        .and_then(|frame| frame.find_window(frame.selected_window))
        .and_then(crate::window::Window::buffer_id);
    if selected_buffer_id == Some(outgoing.buffer_id) {
        return DisconnectedWindowPoint::PreserveSelectedBuffer;
    }

    let last_selected_window = eval
        .edit
        .buffers
        .get(outgoing.buffer_id)
        .and_then(|buffer| buffer.last_selected_window());
    if let Some(window_id) = last_selected_window
        && window_id != outgoing.window_id
        && live_window_displays_buffer(&eval.frames, window_id, outgoing.buffer_id)
    {
        return DisconnectedWindowPoint::PreserveLastSelectedWindow(window_id);
    }

    DisconnectedWindowPoint::CommitWindowPoint(outgoing.window_point)
}

/// Disconnect every current root leaf from its buffer before restoring a
/// saved window tree.  This is the neomacs counterpart of GNU
/// `delete_all_child_windows` calling `unshow_buffer` for each outgoing leaf.
fn unshow_frame_root_buffers(
    eval: &mut super::eval::Context,
    frame_id: neovm_value::display_vocab::FrameId,
) {
    // GNU first swaps the globally selected window's live buffer point into
    // its window marker.  The frame being restored need not be selected.
    if let Some(selected_frame_id) = eval.frames.selected_frame().map(|frame| frame.id) {
        super::window_cmds::remember_selected_window_point_in_state(
            &mut eval.frames,
            &mut eval.edit.buffers,
            selected_frame_id,
        );
    }

    let outgoing_windows = eval
        .frames
        .get(frame_id)
        .map(|frame| {
            frame
                .window_list()
                .into_iter()
                .filter_map(|window_id| match frame.find_window(window_id) {
                    Some(crate::window::Window::Leaf {
                        buffer_id,
                        window_start,
                        point,
                        ..
                    }) => Some(OutgoingWindowBuffer {
                        window_id,
                        buffer_id: *buffer_id,
                        window_start: *window_start,
                        window_point: *point,
                    }),
                    Some(crate::window::Window::Internal { .. }) | None => None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for outgoing in outgoing_windows {
        let point = disconnected_window_point(eval, outgoing);
        if let Some(buffer) = eval.edit.buffers.get_mut(outgoing.buffer_id) {
            buffer.set_last_window_start(outgoing.window_start.max(LispCharPos1::ONE));
        }

        match point {
            DisconnectedWindowPoint::PreserveSelectedBuffer => {}
            DisconnectedWindowPoint::PreserveLastSelectedWindow(window_id) => {
                debug_assert!(live_window_displays_buffer(
                    &eval.frames,
                    window_id,
                    outgoing.buffer_id
                ));
            }
            DisconnectedWindowPoint::CommitWindowPoint(window_point) => {
                let byte_pos = eval.edit.buffers.get(outgoing.buffer_id).map(|buffer| {
                    buffer.lisp_pos_to_emacs_byte_pos(window_point.max(LispCharPos1::ONE))
                });
                if let Some(byte_pos) = byte_pos {
                    let _ = eval
                        .edit
                        .buffers
                        .goto_buffer_emacs_byte_pos(outgoing.buffer_id, byte_pos);
                }
            }
        }

        if let Some(buffer) = eval.edit.buffers.get_mut(outgoing.buffer_id)
            && buffer.last_selected_window() == Some(outgoing.window_id)
        {
            buffer.set_last_selected_window(None);
        }
    }
}

impl WindowConfigurationSnapshot {
    fn clone_for_restore(&self, buffers: &mut crate::buffer::BufferManager) -> Self {
        Self {
            frame_id: self.frame_id,
            change_stamp: self.change_stamp,
            tree:
                crate::window::window_markers::clone_window_tree_with_independent_position_markers(
                    buffers, &self.tree,
                ),
            selected_window: self.selected_window,
            current_buffer: self.current_buffer,
            minibuffer_window: self.minibuffer_window,
            minibuffer_leaf: self.minibuffer_leaf.as_ref().map(|window| {
                crate::window::window_markers::clone_window_leaf_with_independent_position_markers(
                    buffers, window,
                )
            }),
        }
    }

    fn trace_roots(&self, roots: &mut Vec<Value>) {
        self.tree.trace_roots(roots);
        if let Some(minibuffer) = &self.minibuffer_leaf {
            minibuffer.trace_roots(roots);
        }
    }
}

fn window_configuration_snapshot_roots(snapshot: &WindowConfigurationSnapshot) -> Value {
    let mut roots = Vec::new();
    snapshot.trace_roots(&mut roots);
    Value::vector(roots)
}

fn normalize_selected_window_point_in_snapshot(
    snapshot: &mut WindowConfigurationSnapshot,
    buffers: &mut crate::buffer::BufferManager,
) {
    let selected_buffer_id = snapshot
        .tree
        .find(snapshot.selected_window)
        .or_else(|| {
            snapshot
                .minibuffer_leaf
                .as_ref()
                .filter(|window| window.id() == snapshot.selected_window)
        })
        .and_then(|window| window.buffer_id());
    let Some(buffer_id) = selected_buffer_id else {
        return;
    };
    let Some(point) = buffers
        .get(buffer_id)
        .map(|buffer| buffer.point_char_pos().get().saturating_add(1))
    else {
        return;
    };

    let selected = snapshot.selected_window;
    if let Some(window) = snapshot.tree.find_mut(selected) {
        crate::window::window_markers::set_window_point_with_marker(
            buffers,
            window,
            LispCharPos1::from_one_based_usize(point),
        );
        return;
    }

    if let Some(window) = snapshot
        .minibuffer_leaf
        .as_mut()
        .filter(|window| window.id() == snapshot.selected_window)
    {
        crate::window::window_markers::set_window_point_with_marker(
            buffers,
            window,
            LispCharPos1::from_one_based_usize(point),
        );
    }
}

fn persistent_window_parameter_keys(value: Value) -> Vec<Value> {
    let mut keys = Vec::new();
    let mut cursor = value;
    let mut remaining = 1024;
    while cursor.is_cons() && remaining > 0 {
        let entry = cursor.cons_car();
        if entry.is_cons() && entry.cons_cdr().is_truthy() {
            keys.push(entry.cons_car());
        }
        cursor = cursor.cons_cdr();
        remaining -= 1;
    }
    keys
}

fn window_parameter_is_persistent(
    key: &Value,
    persistent_keys: &[Value],
    symbol_mode: SymbolMode,
) -> bool {
    persistent_keys.iter().any(|persistent| {
        crate::emacs_core::value::eq_value_swp(
            key,
            persistent,
            symbol_mode == SymbolMode::Positioned,
        )
    })
}

fn save_persistent_window_parameters(
    window: &mut crate::window::Window,
    persistent_keys: &[Value],
    symbol_mode: SymbolMode,
) {
    let current = window.parameters().clone();
    let saved = persistent_keys
        .iter()
        .map(|key| {
            let value = current
                .iter()
                .find(|(existing_key, _)| {
                    crate::emacs_core::value::eq_value_swp(
                        existing_key,
                        key,
                        symbol_mode == SymbolMode::Positioned,
                    )
                })
                .map(|(_, value)| *value)
                .unwrap_or(Value::NIL);
            (*key, value)
        })
        .collect();
    *window.parameters_mut() = saved;
}

fn save_snapshot_persistent_window_parameters(
    snapshot: &mut WindowConfigurationSnapshot,
    persistent_parameters: Value,
    symbol_mode: SymbolMode,
) -> Vec<Value> {
    let persistent_keys = persistent_window_parameter_keys(persistent_parameters);
    snapshot.tree.for_each_node_mut(|window| {
        save_persistent_window_parameters(window, &persistent_keys, symbol_mode)
    });
    if let Some(minibuffer) = &mut snapshot.minibuffer_leaf {
        save_persistent_window_parameters(minibuffer, &persistent_keys, symbol_mode);
    }
    persistent_keys
}

fn collect_window_parameters(
    tree: &crate::window::WindowTree,
    out: &mut HashMap<neovm_edit::emacs_core::display_metrics::WindowId, Vec<(Value, Value)>>,
) {
    for id in tree.subtree_ids(tree.root_id()) {
        if let Some(window) = tree.find(id) {
            out.insert(id, window.parameters().clone());
        }
    }
}

fn collect_frame_window_parameters(
    frame: &crate::window::Frame,
) -> HashMap<neovm_edit::emacs_core::display_metrics::WindowId, Vec<(Value, Value)>> {
    let mut parameters = HashMap::new();
    collect_window_parameters(frame.tree(), &mut parameters);
    if let Some(minibuffer) = &frame.minibuffer_leaf {
        parameters.insert(minibuffer.id(), minibuffer.parameters().clone());
    }
    parameters
}

fn merge_restored_window_parameters(
    window: &mut crate::window::Window,
    live_parameters: &HashMap<
        neovm_edit::emacs_core::display_metrics::WindowId,
        Vec<(Value, Value)>,
    >,
    symbol_mode: SymbolMode,
) {
    let saved_parameters = window.parameters().clone();
    let saved_keys = saved_parameters
        .iter()
        .map(|(key, _)| *key)
        .collect::<Vec<_>>();
    let live = live_parameters.get(&window.id());
    let mut merged = live
        .into_iter()
        .flat_map(|params| params.iter().copied())
        .filter(|(key, _)| !window_parameter_is_persistent(key, &saved_keys, symbol_mode))
        .collect::<Vec<_>>();

    for (key, saved_value) in saved_parameters {
        if saved_value.is_nil() {
            if live.is_some_and(|params| {
                params.iter().any(|(live_key, live_value)| {
                    crate::emacs_core::value::eq_value_swp(
                        live_key,
                        &key,
                        symbol_mode == SymbolMode::Positioned,
                    ) && live_value.is_truthy()
                })
            }) {
                merged.insert(0, (key, Value::NIL));
            }
        } else {
            merged.insert(0, (key, saved_value));
        }
    }

    *window.parameters_mut() = merged;
}

fn merge_snapshot_window_parameters(
    snapshot: &mut WindowConfigurationSnapshot,
    live_parameters: &HashMap<
        neovm_edit::emacs_core::display_metrics::WindowId,
        Vec<(Value, Value)>,
    >,
    symbol_mode: SymbolMode,
) {
    snapshot.tree.for_each_node_mut(|window| {
        merge_restored_window_parameters(window, live_parameters, symbol_mode)
    });
    if let Some(minibuffer) = &mut snapshot.minibuffer_leaf {
        merge_restored_window_parameters(minibuffer, live_parameters, symbol_mode);
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

fn window_configuration_parts_from_value(value: &Value) -> Option<(Value, i64)> {
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

fn window_configuration_frame_from_value(value: &Value) -> Option<Value> {
    window_configuration_parts_from_value(value).map(|(frame, _)| frame)
}

fn next_window_configuration_serial() -> i64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_WINDOW_CONFIGURATION_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_WINDOW_CONFIGURATION_ID.fetch_add(1, Ordering::Relaxed) as i64
}

fn make_window_configuration_value(frame: Value, serial: i64, roots: Value) -> Value {
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

pub(crate) fn builtin_window_configuration_p(args: Vec<Value>) -> EvalResult {
    expect_args("window-configuration-p", &args, 1)?;
    // GNU: t exactly for the window-configuration pseudovector -- a pure tag
    // check, independent of the saved contents.
    Ok(Value::bool_val(args[0].is_window_configuration()))
}

pub(crate) fn builtin_window_configuration_frame(args: Vec<Value>) -> EvalResult {
    expect_args("window-configuration-frame", &args, 1)?;
    window_configuration_frame_from_value(&args[0]).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-configuration-p"), args[0]],
        )
    })
}

/// GNU `compare_window_configurations` (`src/window.c:9012`) compares the
/// *layout* of two configurations: selected frame, current buffer, window
/// count, and -- per window, in order -- the current-window correspondence,
/// buffer, and geometry/structure.  It deliberately ignores point and scroll.
///
/// neomacs stamps every `current-window-configuration` value with a unique
/// serial, so a raw `equal' (the previous implementation) always returned nil
/// for two distinct-but-identical configurations.  Compare the stored
/// snapshots structurally instead.
fn window_tree_layout_equal(
    tree_a: &crate::window::WindowTree,
    a: neovm_edit::emacs_core::display_metrics::WindowId,
    tree_b: &crate::window::WindowTree,
    b: neovm_edit::emacs_core::display_metrics::WindowId,
) -> bool {
    let (Some(a), Some(b)) = (tree_a.find(a), tree_b.find(b)) else {
        return false;
    };
    window_node_layout_equal(a, b)
        && a.children().len() == b.children().len()
        && a.children()
            .iter()
            .zip(b.children().iter())
            .all(|(x, y)| window_tree_layout_equal(tree_a, *x, tree_b, *y))
}

/// Whether two window nodes were laid out identically, ignoring their
/// children.
fn window_node_layout_equal(a: &crate::window::Window, b: &crate::window::Window) -> bool {
    use crate::window::Window;
    match (a, b) {
        (
            Window::Leaf {
                buffer_id: ba,
                bounds: bounds_a,
                top_line: tla,
                left_col: lca,
                ..
            },
            Window::Leaf {
                buffer_id: bb,
                bounds: bounds_b,
                top_line: tlb,
                left_col: lcb,
                ..
            },
        ) => ba == bb && bounds_a == bounds_b && tla == tlb && lca == lcb,
        (
            Window::Internal {
                direction: da,
                bounds: bounds_a,
                top_line: tla,
                left_col: lca,
                ..
            },
            Window::Internal {
                direction: db,
                bounds: bounds_b,
                top_line: tlb,
                left_col: lcb,
                ..
            },
        ) => da == db && bounds_a == bounds_b && tla == tlb && lca == lcb,
        _ => false,
    }
}

fn window_snapshots_layout_equal(
    a: &WindowConfigurationSnapshot,
    b: &WindowConfigurationSnapshot,
) -> bool {
    a.frame_id == b.frame_id
        && a.current_buffer == b.current_buffer
        // The "current"/selected window must correspond between configurations.
        && a.selected_window == b.selected_window
        && a.minibuffer_window == b.minibuffer_window
        && window_tree_layout_equal(&a.tree, a.tree.root_id(), &b.tree, b.tree.root_id())
        && match (&a.minibuffer_leaf, &b.minibuffer_leaf) {
            (Some(x), Some(y)) => window_node_layout_equal(x, y),
            (None, None) => true,
            _ => false,
        }
}

pub(crate) fn builtin_window_configuration_equal_p(args: Vec<Value>) -> EvalResult {
    expect_args("window-configuration-equal-p", &args, 2)?;
    let Some((_, serial_a)) = window_configuration_parts_from_value(&args[0]) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-configuration-p"), args[0]],
        ));
    };
    let Some((_, serial_b)) = window_configuration_parts_from_value(&args[1]) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-configuration-p"), args[1]],
        ));
    };

    // Identical object (or identical serial) is trivially equal.
    if serial_a == serial_b {
        return Ok(Value::T);
    }

    let result = WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| {
        let store = slot.borrow();
        match (store.get(&serial_a), store.get(&serial_b)) {
            (Some(a), Some(b)) => Some(window_snapshots_layout_equal(a, b)),
            _ => None,
        }
    });

    match result {
        Some(equal) => Ok(Value::bool_val(equal)),
        // Fall back to a structural value comparison if a snapshot was evicted
        // from the bounded side table (very old configurations).
        None => Ok(Value::bool_val(equal_value(&args[0], &args[1], 0))),
    }
}

#[inline(never)]
pub(crate) fn builtin_current_window_configuration(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    expect_max_args("current-window-configuration", &args, 1)?;

    let frame = if let Some(frame) = args.first() {
        crate::emacs_core::window_hooks::expect_optional_live_frame_designator_in_state(
            frame,
            &eval.frames,
        )?;
        if frame.is_nil() {
            super::window_cmds::selected_frame_impl(
                &mut eval.frames,
                &mut eval.edit.buffers,
                vec![],
            )?
        } else {
            *frame
        }
    } else {
        super::window_cmds::selected_frame_impl(&mut eval.frames, &mut eval.edit.buffers, vec![])?
    };

    if !frame.is_frame() {
        return Ok(make_window_configuration_value(
            frame,
            next_window_configuration_serial(),
            Value::vector(Vec::new()),
        ));
    };
    let Some(fid) = frame.as_frame_id() else {
        return Ok(make_window_configuration_value(
            frame,
            next_window_configuration_serial(),
            Value::vector(Vec::new()),
        ));
    };
    let frame_id = neovm_value::display_vocab::FrameId(fid);
    if let Some(frame_state) = eval.frames.get(frame_id) {
        let mut snapshot = WindowConfigurationSnapshot {
            frame_id,
            change_stamp: frame_state.change_stamp,
            tree:
                crate::window::window_markers::clone_window_tree_with_independent_position_markers(
                    &mut eval.edit.buffers,
                    frame_state.tree(),
                ),
            selected_window: frame_state.selected_window,
            current_buffer: eval.edit.buffers.current_buffer_id(),
            minibuffer_window: frame_state.minibuffer_window,
            minibuffer_leaf: frame_state.minibuffer_leaf.as_ref().map(|window| {
                crate::window::window_markers::clone_window_leaf_with_independent_position_markers(
                    &mut eval.edit.buffers,
                    window,
                )
            }),
        };
        normalize_selected_window_point_in_snapshot(&mut snapshot, &mut eval.edit.buffers);
        save_snapshot_persistent_window_parameters(
            &mut snapshot,
            eval.variable_value_by_name("window-persistent-parameters")
                .unwrap_or(Value::NIL),
            symbol_mode,
        );
        let roots = window_configuration_snapshot_roots(&snapshot);
        let serial = next_window_configuration_serial();
        WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| {
            let mut store = slot.borrow_mut();
            store.insert(serial, snapshot);
            if store.len() > 4096
                && let Some(oldest) = store.keys().min().copied()
            {
                store.remove(&oldest);
            }
        });
        return Ok(make_window_configuration_value(frame, serial, roots));
    }

    Ok(make_window_configuration_value(
        frame,
        next_window_configuration_serial(),
        Value::vector(Vec::new()),
    ))
}

pub(crate) fn builtin_set_window_configuration(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("set-window-configuration", &args, 1, 3)?;
    let options = WindowConfigurationRestoreOptions::from_lisp_args(&args);
    set_window_configuration_with_options(eval, args[0], options)
}

/// The one window whose point a restore must NOT take from the snapshot.
///
/// `current-window-configuration` deliberately does not record point in the
/// buffer that was current when it ran ("does not save the value of point in
/// the current buffer", `Fcurrent_window_configuration` docstring,
/// `src/window.c`).  `Fset_window_configuration` honours that by computing
/// `old_point` from the LIVE session before touching anything
/// (`src/window.c:7692-7733`) and writing it back over the window that was
/// selected when the configuration was saved, once the saved tree is installed
/// (`src/window.c:7978-7984`):
///
/// ```c
///   /* Arrange *not* to restore point in the buffer that was
///      current when the window configuration was saved.  */
///   if (EQ (XWINDOW (data->current_window)->contents, new_current_buffer))
///     set_marker_restricted (XWINDOW (data->current_window)->pointm,
///                            make_fixnum (old_point),
///                            XWINDOW (data->current_window)->contents);
/// ```
///
/// Restoring that window's saved point instead rewinds an in-progress session:
/// Helm selects its own window and reads the minibuffer, so every Helm exit
/// restored the selection to wherever it stood when the prompt opened, and the
/// exit action then resolved the wrong source (DIVERGENCES.md 114).
///
/// `Some` carries the whole decision -- window, the buffer that window must
/// still show for the rule to apply, and the live point -- so no call site can
/// apply half of it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LiveCurrentBufferPoint {
    window: neovm_edit::emacs_core::display_metrics::WindowId,
    buffer: crate::buffer::BufferId,
    point: LispCharPos1,
}

impl LiveCurrentBufferPoint {
    /// GNU's `old_point` computation, read from the live session.
    fn read(
        eval: &super::eval::Context,
        snapshot: &WindowConfigurationSnapshot,
    ) -> Option<LiveCurrentBufferPoint> {
        // `new_current_buffer`: the buffer current at save time, unless it has
        // since been killed (GNU drops it then, and with it the whole rule).
        let saved_current_buffer = snapshot.current_buffer?;
        eval.edit.buffers.get(saved_current_buffer)?;

        let frame = eval.frames.get(snapshot.frame_id)?;
        // A dead saved-selected window has nil contents in GNU, so it can never
        // match `new_current_buffer`; `None` here takes the same branches.
        let saved_selected = frame
            .find_window(snapshot.selected_window)
            .or_else(|| {
                frame
                    .minibuffer_leaf
                    .as_ref()
                    .filter(|window| window.id() == snapshot.selected_window)
            })
            .and_then(|window| match window {
                crate::window::Window::Leaf {
                    buffer_id, point, ..
                } => Some((*buffer_id, *point)),
                crate::window::Window::Internal { .. } => None,
            });
        let saved_selected_shows_it =
            saved_selected.is_some_and(|(buffer_id, _)| buffer_id == saved_current_buffer);

        let live_selected_window = eval
            .frames
            .selected_frame()
            .map(|frame| frame.selected_window);
        let live_selected_is_saved_selected =
            live_selected_window == Some(snapshot.selected_window);
        let live_selected_buffer = eval.frames.selected_frame().and_then(|frame| {
            frame
                .find_window(frame.selected_window)
                .or(frame.minibuffer_leaf.as_ref())
                .and_then(|window| window.buffer_id())
        });

        let buffer_point = |buffer_id: crate::buffer::BufferId| {
            eval.edit.buffers.get(buffer_id).map(|buffer| {
                LispCharPos1::from_one_based_usize(buffer.point_char_pos().get().saturating_add(1))
            })
        };

        let point = if eval.edit.buffers.current_buffer_id() == Some(saved_current_buffer) {
            if saved_selected_shows_it
                && live_selected_buffer == Some(saved_current_buffer)
                && !live_selected_is_saved_selected
            {
                saved_selected.map(|(_, point)| point)?
            } else {
                buffer_point(saved_current_buffer)?
            }
        } else if saved_selected_shows_it && !live_selected_is_saved_selected {
            saved_selected.map(|(_, point)| point)?
        } else {
            buffer_point(saved_current_buffer)?
        };

        Some(LiveCurrentBufferPoint {
            window: snapshot.selected_window,
            buffer: saved_current_buffer,
            point,
        })
    }

    /// Write the live point back, but only while the restored window really
    /// does show the buffer that was current at save time -- GNU's
    /// `EQ (XWINDOW (data->current_window)->contents, new_current_buffer)`
    /// guard, evaluated against the freshly restored tree.
    fn apply(self, eval: &mut super::eval::Context, frame_id: neovm_value::display_vocab::FrameId) {
        let buffers = &mut eval.edit.buffers;
        let Some(frame) = eval.frames.get_mut(frame_id) else {
            return;
        };
        let window = match frame.find_window_mut(self.window) {
            Some(window) => window,
            None => match frame
                .minibuffer_leaf
                .as_mut()
                .filter(|window| window.id() == self.window)
            {
                Some(window) => window,
                None => return,
            },
        };
        if window.buffer_id() != Some(self.buffer) {
            return;
        }
        crate::window::window_markers::set_window_point_with_marker(buffers, window, self.point);
    }
}

trait OptionalLiveCurrentBufferPoint {
    fn apply(self, eval: &mut super::eval::Context, frame_id: neovm_value::display_vocab::FrameId);
}

impl OptionalLiveCurrentBufferPoint for Option<LiveCurrentBufferPoint> {
    fn apply(self, eval: &mut super::eval::Context, frame_id: neovm_value::display_vocab::FrameId) {
        if let Some(live) = self {
            live.apply(eval, frame_id);
        }
    }
}

pub(crate) fn set_window_configuration_with_options(
    eval: &mut super::eval::Context,
    configuration: Value,
    options: WindowConfigurationRestoreOptions,
) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    let Some((_frame, serial)) = window_configuration_parts_from_value(&configuration) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-configuration-p"), configuration],
        ));
    };

    let mut focus_redirect_result = Ok(Value::NIL);
    let snapshot = WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| {
        slot.borrow()
            .get(&serial)
            .map(|snapshot| snapshot.clone_for_restore(&mut eval.edit.buffers))
    });

    if let Some(snapshot) = snapshot {
        let selected_frame_before_restore = eval.frames.selected_frame().map(|frame| frame.id);
        let live_parameters = eval
            .frames
            .get(snapshot.frame_id)
            .map(collect_frame_window_parameters)
            .unwrap_or_default();
        let mut snapshot = snapshot;
        // Read the live point BEFORE anything is restored: GNU computes
        // `old_point` at the very top of `Fset_window_configuration`, from the
        // session as it stands (`src/window.c:7692-7733`).
        let live_current_buffer_point = LiveCurrentBufferPoint::read(eval, &snapshot);
        merge_snapshot_window_parameters(&mut snapshot, &live_parameters, symbol_mode);
        prepare_saved_window_buffer_restoration(eval, &mut snapshot);
        prepare_reused_window_histories(eval, &mut snapshot)?;
        window_configuration_redisplay::prepare(eval, &mut snapshot, options);
        // GNU fset_redisplay precedes restoring the window tree.
        eval.gnu_mark_frame_redisplay(snapshot.frame_id);
        unshow_frame_root_buffers(eval, snapshot.frame_id);
        if let Some(frame) = eval.frames.get_mut(snapshot.frame_id) {
            // GNU set-window-configuration disconnects/reconnects root-tree
            // leaves through wset_buffer; adjust_window_count invalidates their
            // base_line_pos. A saved tree must never replay an earlier counter.
            snapshot
                .tree
                .for_each_node_mut(|window| window.set_mode_line_base(None));
            // GNU's owned minibuffer is the root's next sibling (frame.c);
            // delete_all_child_windows traverses siblings too. Native storage
            // keeps that owned leaf separately, so clear both possible copies.
            if let Some(live_mini) = frame.minibuffer_leaf.as_mut() {
                live_mini.set_mode_line_base(None);
            }
            if let Some(saved_mini) = snapshot.minibuffer_leaf.as_mut() {
                saved_mini.set_mode_line_base(None);
            }
            frame.set_tree(snapshot.tree);
            // GNU `Fset_window_configuration` does NOT touch
            // `frame->old_selected_window` directly — that field
            // is updated by `window_change_record` from the next
            // `run_window_change_functions` cycle. neomacs's
            // analog is `frame_window_hook_record_from_live_state`.
            frame.selected_window = snapshot.selected_window;
            if options.minibuffer_window == MinibufferWindowRestoration::RestoreSaved {
                frame.minibuffer_window = snapshot.minibuffer_window;
                frame.minibuffer_leaf = snapshot.minibuffer_leaf;
            } else if crate::emacs_core::eval::gnu_redisplay_hooks_enabled()
                && snapshot
                    .minibuffer_leaf
                    .as_ref()
                    .is_some_and(|mini| frame.minibuffer_window == Some(mini.id()))
            {
                // DONT-SET-MINIWINDOW keeps its live source, not its geometry.
                // `prepare` preserved that source on the restored saved leaf.
                frame.minibuffer_leaf = snapshot.minibuffer_leaf;
            }
        }
        eval.frames.mark_window_topology_changed();
        // GNU `Fset_window_configuration` (window.c) substitutes a live buffer via
        // `other_buffer_safely` for any restored window whose saved buffer was
        // killed, instead of leaving a dead buffer in the window (which would
        // signal "Selecting deleted buffer" on the next redisplay).
        let dead_leaf_windows: Vec<neovm_edit::emacs_core::display_metrics::WindowId> = eval
            .frames
            .get(snapshot.frame_id)
            .map(|frame| {
                frame
                    .window_list()
                    .into_iter()
                    .filter_map(|wid| match frame.find_window(wid) {
                        Some(crate::window::Window::Leaf { buffer_id, .. }) => {
                            Some((wid, *buffer_id))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, buffer_id)| eval.edit.buffers.get(*buffer_id).is_none())
            .map(|(wid, _)| wid)
            .collect();
        if !dead_leaf_windows.is_empty() {
            let avoid = eval
                .edit
                .buffers
                .current_buffer_id()
                .map(Value::make_buffer)
                .unwrap_or(Value::NIL);
            let substitute = super::super::buffer_ops::other_buffer_impl_in_state(
                &mut eval.frames,
                &mut eval.edit.buffers,
                vec![avoid, Value::NIL, Value::NIL],
            )
            .ok()
            .and_then(|value| value.as_buffer_id());
            if let Some(substitute) = substitute {
                let substitute_point = eval
                    .edit
                    .buffers
                    .get(substitute)
                    .map(|buffer| {
                        LispCharPos1::from_one_based_usize(
                            buffer.point_char_pos().get().saturating_add(1),
                        )
                    })
                    .unwrap_or(LispCharPos1::ONE);
                let (frames, buffers) = (&mut eval.frames, &mut eval.edit.buffers);
                if let Some(frame) = frames.get_mut(snapshot.frame_id) {
                    for wid in dead_leaf_windows {
                        if let Some(window) = frame.find_window_mut(wid) {
                            if let crate::window::Window::Leaf {
                                buffer_id,
                                window_start,
                                position_markers,
                                point,
                                old_point,
                                ..
                            } = window
                            {
                                *buffer_id = substitute;
                                *window_start = LispCharPos1::ONE;
                                *point = substitute_point;
                                *old_point = substitute_point;
                                *position_markers =
                                    crate::window::WindowPositionMarkerState::Detached;
                            }
                            crate::window::window_markers::attach_window_position_markers(
                                buffers, window,
                            );
                        }
                    }
                }
            }
        }
        // GNU: "Arrange *not* to restore point in the buffer that was current
        // when the window configuration was saved" (`src/window.c:7978-7984`),
        // which it does after installing the saved tree and before selecting
        // the saved window -- so the buffer point derived from that window
        // below sees the live position too.
        live_current_buffer_point.apply(eval, snapshot.frame_id);
        let selected_window_state = eval.frames.get(snapshot.frame_id).and_then(|frame| {
            frame
                .find_window(frame.selected_window)
                .and_then(|window| match window {
                    crate::window::Window::Leaf {
                        buffer_id, point, ..
                    } => Some((*buffer_id, *point)),
                    crate::window::Window::Internal { .. } => None,
                })
        });
        if let Some((buffer_id, point)) = selected_window_state
            && let Some(buffer) = eval.edit.buffers.get(buffer_id)
        {
            let byte_pos = buffer.lisp_pos_to_emacs_byte_pos(point);
            let _ = eval
                .edit
                .buffers
                .goto_buffer_emacs_byte_pos(buffer_id, byte_pos);
        }
        if let Some(buffer_id) = snapshot.current_buffer {
            if eval.edit.buffers.get(buffer_id).is_some() {
                eval.set_current_buffer_unrecorded(buffer_id)?;
            } else if let Some((buffer_id, _)) = selected_window_state {
                eval.set_current_buffer_unrecorded(buffer_id)?;
            }
        } else if let Some((buffer_id, _)) = selected_window_state {
            eval.set_current_buffer_unrecorded(buffer_id)?;
        }

        // GNU `Fset_window_configuration` restores the saved tree first, then
        // calls `adjust_frame_size` before returning.  The adjustment matters
        // for the initial batch frame: its saved root may still cover all 24
        // pre-menu-bar rows, while the live frame has a one-line top margin and
        // therefore a 23-line root at row 1.
        window_configuration_geometry::reconcile(eval, snapshot.frame_id)?;

        eval.gnu_mark_frame_window_change(snapshot.frame_id);
        let frame_to_select = match options.selected_frame {
            SelectedFrameRestoration::RestoreSaved => Some(snapshot.frame_id),
            SelectedFrameRestoration::KeepSelected => selected_frame_before_restore,
        };
        if let Some(frame_id) = frame_to_select {
            let _ = eval.frames.select_frame(frame_id);
        }
        // The restored configuration is accepted; a host that cannot apply
        // its focus redirects must not cancel the redisplay request below.
        focus_redirect_result = crate::emacs_core::frame::sync_gui_frame_focus_redirects(eval);
    }

    // GNU `Fset_window_configuration` marks the frame for redisplay with
    // `fset_redisplay`; it never performs redisplay (or invokes window-change
    // hooks) synchronously.  Besides preserving the public primitive's phase
    // boundary, deferral is essential while `read_minibuf` unwinds: the outer
    // interactive command must get to install its target before the restored
    // caller configuration can be published.  The typed reason crosses both
    // Neomacs gates that correspond to GNU's `windows_or_buffers_changed`:
    // general redisplay and menu preparation.  Window-change hooks remain
    // owned by `redisplay_with_force`, where the live-vs-recorded frame state
    // is compared exactly once per natural redisplay cycle.
    eval.request_menu_bar_rebuild(super::eval::MenuBarRebuildReason::WindowsOrBuffersChanged);
    focus_redirect_result?;
    Ok(Value::T)
}

/// The symbol `features', interned once.
fn features_symbol_id() -> crate::emacs_core::intern::SymId {
    static ID: std::sync::OnceLock<crate::emacs_core::intern::SymId> = std::sync::OnceLock::new();
    *ID.get_or_init(|| crate::emacs_core::intern::intern("features"))
}

pub(crate) fn builtin_featurep(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
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
        .variable_value(features_symbol_id())
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
        .get_property_id(sym_id, crate::emacs_core::intern::intern("subfeatures"))
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
    crate::tls_fingerprint::mix(
        h,
        WINDOW_CONFIGURATION_SNAPSHOTS
            .with(|slot| slot.installed_heap_identity().unwrap_or(0) as u64),
    );
}
