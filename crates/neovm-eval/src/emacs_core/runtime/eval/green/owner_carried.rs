//! GNU-global state that warm paths read (round 3w, plan-3w.md section 10):
//! the profiler, which every safe point polls; the insertion hooks and the
//! deferred after-change list, which every buffer change reads; and the
//! redisplay change marks, which every buffer change bumps. The running
//! owner keeps it inline in its Context, at the offsets those paths were
//! compiled for, and hands it on with the Baton: a park stows it in the
//! runtime, a resume or a new thread's start takes it back. Every Lisp
//! thread therefore sees one value, as in GNU, and a Sim World never moves
//! it. (N-mutator mode needs shared storage instead: a recorded obligation.)

use super::super::Context;
use super::super::profiler::ProfilerState;
use super::super::redisplay_hooks::RedisplayHookOwnership;
use crate::emacs_core::eval::data::window_state::ChromeDirty;
use neovm_edit::buffer::BufferId;
use neovm_edit::emacs_core::display_metrics::WindowId;
use neovm_value::value::Value;
use rustc_hash::FxHashMap;

/// The warm GNU-global state while its holder is parked.
pub(crate) struct OwnerCarried {
    /// profiler.c's logs and switches.
    profiler: ProfilerState,
    /// buffer.c `last_overlay_modification_hooks`.
    last_overlay_modification_hooks: Vec<neovm_edit::buffer::overlay::OverlayModificationHook>,
    /// textprop.c `interval_insert_behind_hooks`.
    interval_insert_behind_hooks: Value,
    /// textprop.c `interval_insert_in_front_hooks`.
    interval_insert_in_front_hooks: Value,
    /// insdel.c `combine_after_change_list`.
    combine_after_change_list: Vec<(i64, i64, i64)>,
    /// insdel.c `combine_after_change_buffer`.
    combine_after_change_buffer: Option<BufferId>,
    /// xfaces.c `face_change_count`.
    face_change_count: u64,
    /// xdisp.c display-variable change tracking.
    display_var_change_count: u64,
    /// xdisp.c redisplay generation.
    redisplay_generation: u64,
    /// xdisp.c `windows_or_buffers_changed`-style body revision.
    body_redisplay_all: u64,
    /// xdisp.c per-window redisplay revisions.
    body_redisplay_by_window: FxHashMap<WindowId, u64>,
    /// xdisp.c per-buffer redisplay revisions.
    body_redisplay_by_buffer: FxHashMap<BufferId, u64>,
    /// xdisp.c chrome (mode/header/tab line) dirtiness.
    chrome_dirty: ChromeDirty,
    /// xdisp.c redisplay hook ownership (`redisplay` marks).
    gnu_redisplay_hooks: RedisplayHookOwnership,
}

impl OwnerCarried {
    /// Take the state out of OWNER, leaving a fresh owner's values.
    #[cold]
    #[doc(hidden)]
    pub fn take(owner: &mut Context) -> Self {
        use std::mem::{replace, take};
        Self {
            profiler: take(&mut owner.profiler),
            last_overlay_modification_hooks: take(owner.edit.last_overlay_modification_hooks_mut()),
            interval_insert_behind_hooks: replace(
                &mut owner.edit.interval_insert_behind_hooks(),
                Value::NIL,
            ),
            interval_insert_in_front_hooks: replace(
                &mut owner.edit.interval_insert_in_front_hooks(),
                Value::NIL,
            ),
            combine_after_change_list: take(owner.edit.combine_after_change_list_mut()),
            combine_after_change_buffer: take(owner.edit.combine_after_change_buffer_mut()),
            face_change_count: take(&mut owner.face_change_count),
            display_var_change_count: take(&mut owner.display_var_change_count),
            redisplay_generation: take(&mut owner.redisplay_generation),
            body_redisplay_all: take(&mut owner.body_redisplay_all),
            body_redisplay_by_window: take(&mut owner.body_redisplay_by_window),
            body_redisplay_by_buffer: take(&mut owner.body_redisplay_by_buffer),
            chrome_dirty: take(&mut owner.chrome_dirty),
            gnu_redisplay_hooks: replace(
                &mut owner.gnu_redisplay_hooks,
                RedisplayHookOwnership::initial(),
            ),
        }
    }

    /// Put the state into OWNER, which now runs on this OS thread.
    #[cold]
    pub(crate) fn put(self, owner: &mut Context) {
        owner.profiler = self.profiler;
        owner.profiler.resume_on_this_thread();
        owner
            .edit
            .set_last_overlay_modification_hooks(self.last_overlay_modification_hooks);
        owner
            .edit
            .set_interval_insert_behind_hooks(self.interval_insert_behind_hooks);
        owner
            .edit
            .set_interval_insert_in_front_hooks(self.interval_insert_in_front_hooks);
        owner
            .edit
            .set_combine_after_change_list(self.combine_after_change_list);
        owner
            .edit
            .set_combine_after_change_buffer(self.combine_after_change_buffer);
        owner.face_change_count = self.face_change_count;
        owner.display_var_change_count = self.display_var_change_count;
        owner.redisplay_generation = self.redisplay_generation;
        owner.body_redisplay_all = self.body_redisplay_all;
        owner.body_redisplay_by_window = self.body_redisplay_by_window;
        owner.body_redisplay_by_buffer = self.body_redisplay_by_buffer;
        owner.chrome_dirty = self.chrome_dirty;
        owner.gnu_redisplay_hooks = self.gnu_redisplay_hooks;
    }

    /// Visit every value the stowed state holds.
    #[doc(hidden)]
    pub fn trace_roots(&self, visit: &mut dyn FnMut(Value)) {
        self.profiler.trace_roots(visit);
        for hook in &self.last_overlay_modification_hooks {
            visit(hook.hook_list());
            visit(hook.overlay());
        }
        visit(self.interval_insert_behind_hooks);
        visit(self.interval_insert_in_front_hooks);
    }
}
