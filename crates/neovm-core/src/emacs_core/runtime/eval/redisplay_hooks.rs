//! GNU redisplay target ownership and transaction boundary.
//!
//! State is owned by one Context and accessed under its exclusive mutator
//! borrow. It is neither process-global Lisp state nor a thread-local cache;
//! independent mutators own independent flags. IDs/points contain no Lisp
//! objects. The exceptional layout Flow is an owned, already-rooted Flow
//! (its payload uses InFlightRoots), consumed at the enclosing transaction.

mod echo_message;

use super::*;
use crate::buffer::{BufferId, LispCharPos1};
use crate::window::{FrameId, Window, WindowId};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::sync::OnceLock;

/// A renderer-inert GNU resize_mini_window request. The source buffer is
/// explicit because inactive echo uses a temporary buffer, while the selected
/// minibuffer uses its live buffer even before a reader is active. No body/chrome presentation is accepted.
/// Each exclusive Context request owns a copy of this immutable numeric selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedisplayMiniGeometrySource {
    EchoArea,
    ActiveMinibuffer,
}

/// Numeric preparation request passed under the exclusive Context/frontend borrow.
/// Independent mutators create their own requests; IDs and copied policy retain
/// no Lisp values or shared mutable preparation state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RedisplayMiniGeometryRequest {
    pub frame: FrameId,
    pub window: WindowId,
    pub buffer: BufferId,
    pub source: RedisplayMiniGeometrySource,
    pub exact: bool,
}

/// Immutable process policy published by OnceLock for concurrent numeric reads.
/// Independent Context mutators share only this selector, with no Lisp-state cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RedisplayHookPolicy {
    #[default]
    Legacy,
    Gnu,
}

pub(super) fn parse_redisplay_hooks(value: Option<&OsStr>) -> RedisplayHookPolicy {
    match value.and_then(OsStr::to_str) {
        Some("on" | "1" | "true" | "yes") => RedisplayHookPolicy::Gnu,
        _ => RedisplayHookPolicy::Legacy,
    }
}

#[inline]
pub(crate) fn gnu_redisplay_hooks_enabled() -> bool {
    #[cfg(test)]
    if let Some(policy) = TEST_REDISPLAY_HOOK_POLICY.with(std::cell::Cell::get) {
        return policy == RedisplayHookPolicy::Gnu;
    }
    static POLICY: OnceLock<RedisplayHookPolicy> = OnceLock::new();
    *POLICY.get_or_init(|| {
        parse_redisplay_hooks(std::env::var_os("NEOMACS_REDISPLAY_GNU_HOOKS").as_deref())
    }) == RedisplayHookPolicy::Gnu
}

#[cfg(test)]
thread_local! {
    /// Numeric test policy only, independently owned by each test thread. It
    /// holds no Lisp values, caches or runtime state and does not exist in
    /// production or alter the process-once production policy lookup.
    static TEST_REDISPLAY_HOOK_POLICY: std::cell::Cell<Option<RedisplayHookPolicy>> = const {
        std::cell::Cell::new(None)
    };
}

/// Restore a numeric fixture selector on normal return and Rust unwinding.
/// The !Send/!Sync marker keeps this guard on its owning test mutator thread;
/// independent test threads share no mutable selector or Lisp state.
#[cfg(test)]
pub(crate) struct RedisplayHookPolicyGuard {
    previous: Option<RedisplayHookPolicy>,
    _same_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(test)]
impl RedisplayHookPolicyGuard {
    pub(crate) fn legacy() -> Self {
        Self::set(RedisplayHookPolicy::Legacy)
    }

    pub(crate) fn gnu() -> Self {
        Self::set(RedisplayHookPolicy::Gnu)
    }

    fn set(policy: RedisplayHookPolicy) -> Self {
        Self {
            previous: TEST_REDISPLAY_HOOK_POLICY.with(|slot| slot.replace(Some(policy))),
            _same_thread: std::marker::PhantomData,
        }
    }
}

#[cfg(test)]
impl Drop for RedisplayHookPolicyGuard {
    fn drop(&mut self) {
        TEST_REDISPLAY_HOOK_POLICY.with(|slot| slot.set(self.previous));
    }
}

/// Numeric pending-work scope updated only through its owning Context borrow.
/// Independent mutators own separate scopes and share no mutable demand state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PendingScope {
    #[default]
    None,
    Some,
    All,
}

impl PendingScope {
    fn raise(&mut self, scope: Self) {
        *self = (*self).max(scope);
    }
}

/// Numeric dimension record stored by one exclusively borrowed Context. Copies
/// are immutable observations; independent mutators share no mutable record or Lisp state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HookWindowDimensions {
    pub total_width: f32,
    pub total_height: f32,
    pub body_width: i64,
    pub body_height: i64,
}

/// The existing one-byte reentry slot distinguishes a complete GNU redisplay
/// from a standalone committed-start callback. Both inhibit recursive display;
/// only the complete transaction owns pre-layout mini geometry preparation.
/// One exclusive Context mutator owns this numeric value; independent contexts
/// share no mutable selector or Lisp state through it.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RedisplayActiveOwner {
    #[default]
    Idle,
    RedisplayTransaction,
    CommittedScroll,
}

impl RedisplayActiveOwner {
    #[inline]
    const fn is_active(self) -> bool {
        !matches!(self, Self::Idle)
    }
}

const _: () = {
    assert!(std::mem::size_of::<RedisplayActiveOwner>() == std::mem::size_of::<bool>());
    assert!(std::mem::align_of::<RedisplayActiveOwner>() == std::mem::align_of::<bool>());
};

/// Transient display ownership; never dumped as an active transaction.
/// Publication and acknowledgements require exclusive Context access. The
/// process-once policy is immutable and safe to read from every mutator.
#[derive(Default)]
pub(crate) struct RedisplayHookOwnership {
    active: RedisplayActiveOwner,
    windows: PendingScope,
    mode_lines: PendingScope,
    redisplay_windows: HashSet<WindowId>,
    mode_line_windows: HashSet<WindowId>,
    redisplay_frames: HashSet<FrameId>,
    physical_redraw_frames: HashSet<FrameId>,
    redisplay_text: HashSet<BufferId>,
    last_points: HashMap<WindowId, LispCharPos1>,
    pub(crate) frame_window_change: HashSet<FrameId>,
    pub(crate) dimensions: HashMap<WindowId, HookWindowDimensions>,
    pub(crate) old_selected_frame: Option<FrameId>,
    pub(crate) old_selected_window: Option<WindowId>,
    layout_flow: Option<Flow>,
    echo_geometry_window: Option<WindowId>,
    accepted_serial: u64,
}

impl RedisplayHookOwnership {
    pub(super) fn initial() -> Self {
        Self {
            windows: PendingScope::All,
            ..Self::default()
        }
    }
    fn request_physical_redraw(&mut self, frame: FrameId) {
        self.physical_redraw_frames.insert(frame);
    }
    #[inline]
    fn take_physical_redraw(&mut self, frame: FrameId) -> bool {
        if self.physical_redraw_frames.is_empty() {
            return false;
        }
        self.physical_redraw_frames.remove(&frame)
    }
    fn acknowledge_frame_target(&mut self, frame: FrameId) {
        self.redisplay_frames.remove(&frame);
        self.accepted_serial = self.accepted_serial.wrapping_add(1);
    }
}

/// A numeric failure reason local to one exclusively owned display attempt.
/// Independent mutators share no state through this immutable enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedisplayMiniPreparationFailure {
    DidNotConverge,
    PresentationBorrowed,
}

impl Context {
    /// Build a rooted failure for the full minibuffer display-motion producer.
    /// The caller exclusively owns this Context; the Flow owns its Lisp payload
    /// through the existing in-flight root contract and adds no shared cache.
    #[cold]
    #[inline(never)]
    pub fn failed_redisplay_mini_preparation(
        &self,
        reason: RedisplayMiniPreparationFailure,
    ) -> Flow {
        let message = match reason {
            RedisplayMiniPreparationFailure::DidNotConverge => {
                "Minibuffer geometry preparation did not converge"
            }
            RedisplayMiniPreparationFailure::PresentationBorrowed => {
                "Minibuffer geometry preparation is already active"
            }
        };
        super::super::error::signal("error", vec![Value::string(message)])
    }

    /// A successful preparatory resize owns the GNU frame geometry/change
    /// flags before window-change hooks; it is not a painted-frame acceptance.
    pub fn gnu_publish_mini_geometry_changed(&mut self, frame: FrameId) {
        if !gnu_redisplay_hooks_enabled() {
            return;
        }
        self.gnu_mark_frame_redisplay(frame);
        self.gnu_mark_frame_window_change(frame);
        self.invalidate_redisplay();
    }

    /// Commit resize_mini_window's start without publishing a scroll callback.
    /// This owns disjoint frame/buffer fields under the exclusive Context;
    /// callers cannot split that borrow across frontend APIs.
    pub fn gnu_set_prepared_minibuffer_start(
        &mut self,
        frame: FrameId,
        window: WindowId,
        start: LispCharPos1,
        _source_screen_line_aligned: bool,
    ) {
        if !gnu_redisplay_hooks_enabled() {
            return;
        }
        if let Some(window) = self
            .frames
            .get_mut(frame)
            .and_then(|frame| frame.find_window_mut(window))
        {
            crate::window::window_markers::set_window_start_with_marker(
                &mut self.buffers,
                window,
                start,
            );
            // GNU resize_mini_window writes w->start and start_at_line_beg;
            // it neither invents nor consumes w->force_start. Preserve a
            // caller's explicit forced start until redisplay_window sees it.
        }
    }

    fn gnu_prune_dead_owners(&mut self) {
        let frames: HashSet<_> = self.gnu_frame_order().into_iter().collect();
        let windows: HashSet<_> = frames
            .iter()
            .flat_map(|frame| self.gnu_window_order(*frame))
            .collect();
        self.gnu_redisplay_hooks
            .redisplay_windows
            .retain(|window| windows.contains(window));
        self.gnu_redisplay_hooks
            .mode_line_windows
            .retain(|window| windows.contains(window));
        self.gnu_redisplay_hooks
            .last_points
            .retain(|window, _| windows.contains(window));
        self.gnu_redisplay_hooks
            .dimensions
            .retain(|window, _| windows.contains(window));
        self.gnu_redisplay_hooks
            .redisplay_frames
            .retain(|frame| frames.contains(frame));
        self.gnu_redisplay_hooks
            .physical_redraw_frames
            .retain(|frame| frames.contains(frame));
        self.gnu_redisplay_hooks
            .frame_window_change
            .retain(|frame| frames.contains(frame));
        self.gnu_redisplay_hooks
            .redisplay_text
            .retain(|buffer| self.buffers.get(*buffer).is_some());
    }

    /// GNU grow_mini_window calls Lisp to stage achievable root sizes, then
    /// commits the staged tree together with mini pixels. It is intentionally
    /// fallible: user overrides of the sizing function can nonlocally exit.
    pub fn gnu_apply_prepared_minibuffer_resize(
        &mut self,
        frame: FrameId,
        window: WindowId,
        requested_height: f32,
    ) -> EvalResult {
        if !gnu_redisplay_hooks_enabled() {
            return Ok(Value::NIL);
        }
        let Some(state) = self.frames.get(frame) else {
            return Ok(Value::NIL);
        };
        let Some(mini) = state.find_window(window) else {
            return Ok(Value::NIL);
        };
        let previous = mini.bounds().height.round() as i64;
        let desired = requested_height.round() as i64;
        let delta = desired.saturating_sub(previous);
        if delta == 0 {
            return Ok(Value::NIL);
        }
        let root = state.root_window().id();
        let is_tty = state.effective_window_system().is_none();
        let was_dirty = self.gnu_redisplay_hooks.redisplay_frames.contains(&frame);
        let grow = self.funcall_general(
            Value::symbol("window--resize-root-window-vertically"),
            vec![Value::make_window(root.0), Value::fixnum(-delta), Value::T],
        )?;
        let Some(grow) = grow.as_fixnum().filter(|grow| *grow != 0) else {
            return Ok(Value::NIL);
        };
        // grow_mini_window quietly refuses unachievable staged root sizes.
        // Validate before the public mini commit primitive, whose direct Lisp
        // entry intentionally signals on invalid staged geometry.
        let Some(state) = self.frames.get(frame) else {
            return Ok(Value::NIL);
        };
        if !crate::window::window_resize_check(state.tree(), state.tree().root_id(), false) {
            return Ok(Value::NIL);
        }
        if let Some(mini) = self
            .frames
            .get_mut(frame)
            .and_then(|frame| frame.find_window_mut(window))
        {
            mini.set_new_pixel(Some(previous.saturating_sub(grow)));
        } else {
            return Ok(Value::NIL);
        }
        let committed = super::super::window_cmds::builtin_resize_mini_window_internal(
            self,
            vec![Value::make_window(window.0)],
        )?;
        if committed.is_truthy() {
            self.gnu_publish_mini_geometry_changed(frame);
            if was_dirty && is_tty {
                self.gnu_request_frame_redraw(frame);
            }
        }
        Ok(committed)
    }

    /// Execute resize_mini_window's mini-only Lisp arm for one selected-mini
    /// or echo request. Callers own the outdated predicate and start reset;
    /// this never scans every visible mini-only frame on idle redisplay.
    pub fn gnu_resize_prepared_mini_frame(&mut self, frame: FrameId) -> EvalResult {
        if !gnu_redisplay_hooks_enabled()
            || !self
                .special_variable_value_by_id(intern("resize-mini-frames"))
                .is_some_and(|value| value.is_truthy())
        {
            return Ok(Value::NIL);
        }
        self.safe_funcall(
            Value::symbol("window--resize-mini-frame"),
            vec![Value::make_frame(frame.0)],
        )
    }

    /// The mini-only arm does not require glyph motion or a renderer producer.
    /// Exclusive Context ownership keeps its dynamic reads and start commit
    /// local to this mutator; the existing safe call owns Lisp Flow/rooting.
    #[cold]
    #[inline(never)]
    fn gnu_prepare_mini_only_without_renderer(
        &mut self,
        frame: FrameId,
        window: WindowId,
    ) -> EvalResult {
        // GNU xdisp.c:13299 checks inhibit before the optional start reset,
        // even when resize-mini-frames itself is nil.
        if self
            .special_variable_value_by_id(intern("inhibit-redisplay"))
            .is_some_and(|value| value.is_truthy())
        {
            return Ok(Value::NIL);
        }
        if self
            .special_variable_value_by_id(intern("redisplay-adhoc-scroll-in-resize-mini-windows"))
            .is_none_or(|value| value.is_truthy())
        {
            let begin = self
                .frames
                .get(frame)
                .and_then(|frame| frame.find_window(window))
                .and_then(Window::buffer_id)
                .and_then(|buffer| self.buffers.get(buffer))
                .map(|buffer| {
                    LispCharPos1::from_one_based_usize(
                        buffer.point_min_char_pos().get().saturating_add(1),
                    )
                });
            if let Some(begin) = begin {
                self.gnu_set_prepared_minibuffer_start(frame, window, begin, false);
            }
        }
        self.gnu_resize_prepared_mini_frame(frame)
    }

    pub(crate) fn gnu_guard_committed_scroll(&mut self) -> CommittedScrollGuard<'_> {
        let previous = self.gnu_redisplay_hooks.active;
        self.gnu_redisplay_hooks.active = if previous.is_active() {
            previous
        } else {
            RedisplayActiveOwner::CommittedScroll
        };
        CommittedScrollGuard {
            eval: self,
            previous,
        }
    }

    pub(crate) fn gnu_mark_window_redisplay(&mut self, window: WindowId) {
        if !gnu_redisplay_hooks_enabled() {
            return;
        }
        self.gnu_redisplay_hooks.redisplay_windows.insert(window);
        if self.gnu_selected_window() != Some(window) {
            self.gnu_redisplay_hooks.windows.raise(PendingScope::Some);
        }
    }

    pub(crate) fn gnu_mark_window_mode_line(&mut self, window: WindowId) {
        if !gnu_redisplay_hooks_enabled() {
            return;
        }
        self.gnu_redisplay_hooks.mode_line_windows.insert(window);
        self.chrome_dirty.mark_window(window);
        self.gnu_mark_window_redisplay(window);
    }

    pub(crate) fn gnu_mark_frame_redisplay(&mut self, frame: FrameId) {
        if !gnu_redisplay_hooks_enabled() {
            return;
        }
        self.gnu_redisplay_hooks.redisplay_frames.insert(frame);
        self.gnu_redisplay_hooks.windows.raise(PendingScope::Some);
    }

    /// FRAME_WINDOW_CHANGE is distinct from fset_redisplay (xdisp.c:879).
    pub(crate) fn gnu_mark_frame_window_change(&mut self, frame: FrameId) {
        if gnu_redisplay_hooks_enabled() {
            self.gnu_redisplay_hooks.frame_window_change.insert(frame);
        }
    }

    /// Publish bset_redisplay at mutation time, before Lisp before-change work.
    /// Counting shared text and comparing actual selected BufferId are different
    /// operations in GNU; indirect buffers must not collapse the latter test.
    pub(crate) fn gnu_mark_buffer_redisplay(&mut self, buffer: BufferId) {
        if !gnu_redisplay_hooks_enabled() {
            return;
        }
        let count = self.frames.buffer_window_count(&self.buffers, buffer);
        if count == 0 {
            return;
        }
        if let Some(root) = self.buffers.shared_text_root_id(buffer) {
            self.gnu_redisplay_hooks.redisplay_text.insert(root);
        }
        let selected_buffer = self
            .frames
            .selected_frame()
            .and_then(|frame| frame.selected_window())
            .and_then(Window::buffer_id);
        if count > 1 || selected_buffer != Some(buffer) {
            self.gnu_redisplay_hooks.windows.raise(PendingScope::Some);
        }
    }

    /// bset_update_mode_line marks shared text even for an undisplayed buffer.
    pub(crate) fn gnu_mark_buffer_chrome_cache(&mut self, buffer: BufferId) {
        for frame in self.gnu_frame_order() {
            for window in self.gnu_window_order(frame) {
                let shown = self
                    .frames
                    .get(frame)
                    .and_then(|frame| frame.find_window(window))
                    .and_then(Window::buffer_id);
                if shown.and_then(|shown| self.buffers.shared_text_root_id(shown))
                    == self.buffers.shared_text_root_id(buffer)
                {
                    self.chrome_dirty.mark_window(window);
                }
            }
        }
    }

    pub(crate) fn gnu_mark_buffer_mode_line(&mut self, buffer: BufferId) {
        if !gnu_redisplay_hooks_enabled() {
            return;
        }
        self.gnu_redisplay_hooks
            .mode_lines
            .raise(PendingScope::Some);
        if let Some(root) = self.buffers.shared_text_root_id(buffer) {
            self.gnu_redisplay_hooks.redisplay_text.insert(root);
        }
        self.gnu_mark_buffer_chrome_cache(buffer);
        self.request_menu_bar_rebuild(MenuBarRebuildReason::UpdateModeLines);
    }

    pub(crate) fn gnu_mark_mode_lines_all(&mut self) {
        if gnu_redisplay_hooks_enabled() {
            self.gnu_redisplay_hooks.mode_lines.raise(PendingScope::All);
        }
    }

    /// Physical terminal damage is owned until the matching prepared frame is
    /// actually rendered. Hook recording/display acceptance cannot consume it.
    /// Explicit repaint is independent of the experimental hook scope policy.
    pub fn gnu_request_frame_redraw(&mut self, frame: FrameId) {
        self.gnu_redisplay_hooks.request_physical_redraw(frame);
    }

    #[inline]
    pub fn gnu_take_tty_frame_redraw(&mut self, frame: FrameId) -> bool {
        self.gnu_redisplay_hooks.take_physical_redraw(frame)
    }

    pub fn gnu_redisplay_hooks_policy_enabled(&self) -> bool {
        gnu_redisplay_hooks_enabled()
    }

    /// True only while the complete GNU transaction owns mini preparation.
    /// A standalone committed-start callback also prevents recursive redisplay,
    /// but it does not own sizing for a later renderer-facing layout attempt.
    #[inline]
    pub fn gnu_redisplay_transaction_active(&self) -> bool {
        self.gnu_redisplay_hooks.active == RedisplayActiveOwner::RedisplayTransaction
    }

    pub(crate) fn gnu_mark_windows_all(&mut self) {
        if gnu_redisplay_hooks_enabled() {
            self.gnu_redisplay_hooks.windows.raise(PendingScope::All);
        }
    }

    /// GNU normal/mark-for-redisplay selection marks old and new windows;
    /// other nonnil NORECORD selections raise SOME without either window mark.
    pub(crate) fn gnu_mark_selection(
        &mut self,
        old: Option<WindowId>,
        new: WindowId,
        mark_windows: bool,
    ) {
        if !gnu_redisplay_hooks_enabled() {
            return;
        }
        if mark_windows {
            if let Some(old) = old {
                self.gnu_mark_window_redisplay(old);
            }
            self.gnu_mark_window_redisplay(new);
        } else {
            self.gnu_redisplay_hooks.windows.raise(PendingScope::Some);
        }
    }

    pub(crate) fn gnu_selected_window(&self) -> Option<WindowId> {
        self.frames
            .selected_frame()
            .map(|frame| frame.selected_window)
    }

    /// GNU Vframe_list is newest-created first. FrameIds are monotonic and are
    /// never reused by FrameManager; its HashMap traversal is not frame order.
    pub(crate) fn gnu_frame_order(&self) -> Vec<FrameId> {
        let mut frames = self.frames.frame_list();
        frames.sort_unstable_by_key(|frame| std::cmp::Reverse(frame.0));
        frames
    }

    pub(crate) fn gnu_window_order(&self, frame: FrameId) -> Vec<WindowId> {
        let Some(frame) = self.frames.get(frame) else {
            return Vec::new();
        };
        let mut windows = frame.window_list();
        if let Some(mini) = frame.minibuffer_leaf.as_ref() {
            if !windows.contains(&mini.id()) {
                windows.push(mini.id());
            }
        }
        windows
    }

    fn gnu_window_point(&self, frame: FrameId, window: WindowId) -> Option<LispCharPos1> {
        let frame = self.frames.get(frame)?;
        let window_state = frame.find_window(window)?;
        if self.gnu_selected_window() == Some(window) {
            self.buffers
                .get(window_state.buffer_id()?)
                .map(|buffer| buffer.point_lisp_char_pos())
        } else {
            match window_state {
                Window::Leaf { point, .. } => Some(*point),
                _ => None,
            }
        }
    }

    fn gnu_pre_targets(&mut self) -> Value {
        let state = &self.gnu_redisplay_hooks;
        if state.windows == PendingScope::All || state.mode_lines == PendingScope::All {
            return Value::T;
        }
        if state.windows == PendingScope::None && state.mode_lines == PendingScope::None {
            return Value::NIL;
        }
        let mut windows = Value::NIL;
        for frame_id in self.gnu_frame_order() {
            for window_id in self.gnu_window_order(frame_id) {
                let Some(window) = self
                    .frames
                    .get(frame_id)
                    .and_then(|frame| frame.find_window(window_id))
                else {
                    continue;
                };
                let Some(buffer) = window.buffer_id() else {
                    continue;
                };
                let state = &self.gnu_redisplay_hooks;
                let dirty = state.redisplay_windows.contains(&window_id)
                    || state.mode_line_windows.contains(&window_id)
                    || state.redisplay_frames.contains(&frame_id)
                    || self
                        .buffers
                        .shared_text_root_id(buffer)
                        .is_some_and(|root| state.redisplay_text.contains(&root))
                    || self.gnu_window_point(frame_id, window_id)
                        != state.last_points.get(&window_id).copied();
                if dirty {
                    windows = Value::cons(Value::make_window(window_id.0), windows);
                }
            }
        }
        windows
    }

    /// Copy shared-text dirty marks before any accurate window clears them.
    fn gnu_distribute_text_marks(&mut self) {
        for frame_id in self.gnu_frame_order() {
            for window_id in self.gnu_window_order(frame_id) {
                let root = self
                    .frames
                    .get(frame_id)
                    .and_then(|frame| frame.find_window(window_id))
                    .and_then(Window::buffer_id)
                    .and_then(|buffer| self.buffers.shared_text_root_id(buffer));
                if root.is_some_and(|root| self.gnu_redisplay_hooks.redisplay_text.contains(&root))
                {
                    self.gnu_redisplay_hooks.redisplay_windows.insert(window_id);
                }
            }
        }
    }

    /// Called only at the layout engine's accepted presentation seal. Queries
    /// and aborted/retried attempts must not acknowledge redisplay ownership.
    pub fn note_gnu_frame_display_accepted(
        &mut self,
        frame: FrameId,
        windows: impl IntoIterator<Item = WindowId>,
    ) {
        if !gnu_redisplay_hooks_enabled() {
            return;
        }
        self.gnu_distribute_text_marks();
        for window in windows {
            // GNU init_iterator raises FRAME_WINDOW_CHANGE when a completed
            // body differs from the dimensions recorded by the last hook pass.
            // Keep that old record: the next redisplay owns callbacks and the
            // replacement epoch. Queries and failed presentation prepares never
            // reach this accepted-output boundary.
            let body_changed = self
                .frames
                .get(frame)
                .filter(|frame| frame.minibuffer_window != Some(window))
                .and_then(|frame| frame.redisplay_snapshot(window))
                .filter(|snapshot| snapshot.regions_materialized)
                .and_then(|_| {
                    crate::emacs_core::window_cmds::hook_window_body_dimensions(
                        &self.frames,
                        &self.buffers,
                        frame,
                        window,
                    )
                    .ok()
                })
                .zip(self.gnu_redisplay_hooks.dimensions.get(&window))
                .is_some_and(|((width, height), old)| {
                    width != old.body_width || height != old.body_height
                });
            if body_changed {
                self.gnu_mark_frame_window_change(frame);
            }
            let Some(point) = self.gnu_window_point(frame, window) else {
                continue;
            };
            let root = self
                .frames
                .get(frame)
                .and_then(|frame| frame.find_window(window))
                .and_then(Window::buffer_id)
                .and_then(|buffer| self.buffers.shared_text_root_id(buffer));
            self.gnu_redisplay_hooks.last_points.insert(window, point);
            self.gnu_redisplay_hooks.redisplay_windows.remove(&window);
            self.gnu_redisplay_hooks.mode_line_windows.remove(&window);
            if let Some(root) = root {
                self.gnu_redisplay_hooks.redisplay_text.remove(&root);
            }
        }
        self.gnu_redisplay_hooks.acknowledge_frame_target(frame);
    }

    /// Exceptional transfer out of a unit-return frontend callback. Flow owns
    /// its payload pin; publication precedes aborted layout, and the enclosing
    /// transaction consumes it before recording display success.
    pub fn defer_redisplay_hook_flow(&mut self, flow: Flow) {
        if self.gnu_redisplay_hooks.layout_flow.is_none() {
            self.gnu_redisplay_hooks.layout_flow = Some(flow);
        }
    }

    pub fn redisplay_hook_flow_pending(&self) -> bool {
        self.gnu_redisplay_hooks.layout_flow.is_some()
    }

    fn gnu_mini_geometry_request(&mut self) -> Option<RedisplayMiniGeometryRequest> {
        let selected = self.frames.selected_frame()?;
        let mini = selected.minibuffer_window?;
        let selected_window = self.gnu_selected_window();
        // GNU xdisp.c:17525 permits a cleared echo source only at reader
        // depth zero when the selected window is not a mini window. A live
        // message may still own echo while the reader is active.
        let cleared_echo_allowed = !self.minibuffer_is_active() && selected_window != Some(mini);
        if self.has_current_message()
            || (cleared_echo_allowed && self.gnu_redisplay_hooks.echo_geometry_window.is_some())
        {
            let window = if self.has_current_message() {
                mini
            } else {
                self.gnu_redisplay_hooks.echo_geometry_window?
            };
            let frame = self.frames.find_window_frame_id(window)?;
            self.ensure_echo_area_buffers();
            return Some(RedisplayMiniGeometryRequest {
                frame,
                window,
                buffer: self.echo_area_display_buffer()?,
                source: RedisplayMiniGeometrySource::EchoArea,
                exact: self.echo_area_resize_exact_pending,
            });
        }
        // GNU xdisp.c:17548 tests the selected global mini window, without a
        // minibuf_level requirement. A selected mini-only root can therefore
        // need sizing before any reader/echo source has become active.
        let window = selected_window?;
        if window != mini {
            return None;
        }
        let frame = self.frames.find_window_frame_id(window)?;
        let buffer = self.frames.get(frame)?.find_window(window)?.buffer_id()?;
        // GNU window_outdated compares source revisions, not point alone.
        let current = self.buffer_layout_inputs(buffer)?;
        let old = self
            .last_redisplay_signature
            .as_ref()
            .and_then(|signature| signature.frame.as_ref())
            .and_then(|frame| {
                frame
                    .windows
                    .iter()
                    .find(|entry| entry.layout.id.0 == window.0)
            })
            .and_then(|window| window.buffer.as_ref())
            .map(|buffer| &buffer.layout);
        if old == Some(&current) && !self.gnu_redisplay_hooks.redisplay_windows.contains(&window) {
            return None;
        }
        Some(RedisplayMiniGeometryRequest {
            frame,
            window,
            buffer,
            source: RedisplayMiniGeometrySource::ActiveMinibuffer,
            exact: false,
        })
    }

    fn gnu_run_pre_redisplay(&mut self) -> EvalResult {
        let Some(function) = self.special_variable_value_by_id(intern("pre-redisplay-function"))
        else {
            return Ok(Value::NIL);
        };
        if !builtins::types::value_is_function(self, function) {
            return Ok(Value::NIL);
        }
        let roots = self.save_specpdl_roots();
        self.push_specpdl_root(function);
        let windows = self.gnu_pre_targets();
        self.push_specpdl_root(windows);
        let count = self.specpdl.len();
        let result = (|| {
            self.try_specbind_or_unwind_to(count, intern("inhibit-quit"), Value::T)?;
            self.safe_funcall(function, vec![windows])
        })();
        let result = self.unbind_to_with_result(count, result);
        self.restore_specpdl_roots(roots);
        result
    }

    pub(crate) fn redisplay_with_force_flow(&mut self, force: bool) -> EvalResult {
        if let Some(flow) = self.take_mode_line_display_flow() {
            return Err(flow);
        }
        if !gnu_redisplay_hooks_enabled() {
            self.redisplay_with_force_legacy(force)?;
            return Ok(Value::NIL);
        }
        if self.gnu_redisplay_hooks.active.is_active()
            || self
                .special_variable_value_by_id(intern("inhibit-redisplay"))
                .is_some_and(|value| value.is_truthy())
            || self.redisplay_fn.is_none()
        {
            return Ok(Value::NIL);
        }
        self.gnu_redisplay_hooks.active = RedisplayActiveOwner::RedisplayTransaction;
        let binding_count = self.specpdl.len();
        let restrictions = self.buffers.reset_outermost_restrictions();
        let mut guard = RedisplayTransaction {
            eval: self,
            restrictions: Some(restrictions),
            callback: None,
            preparation: None,
            binding_count,
        };
        guard.run(force)
    }
}

/// Owns the exclusive borrow through pre, change hooks, frontend retries and
/// acceptance. Drop restores transient restrictions, callback and reentry even
/// during Rust unwinding; no raw pointer or single-mutator global is needed.
struct RedisplayTransaction<'a> {
    eval: &'a mut Context,
    restrictions: Option<crate::buffer::buffer::OutermostRestrictionResetState>,
    callback: Option<Box<dyn FnMut(&mut Context)>>,
    preparation: Option<Box<dyn FnMut(&mut Context, RedisplayMiniGeometryRequest) -> EvalResult>>,
    binding_count: usize,
}

impl RedisplayTransaction<'_> {
    fn run(&mut self, force: bool) -> EvalResult {
        self.eval.sync_pending_resize_events();
        self.eval.gnu_prune_dead_owners();
        if let Some(buffer) = self.eval.buffers.current_buffer() {
            crate::window::window_markers::sync_all_frames_for_buffer(
                &mut self.eval.frames,
                buffer.id,
            );
        }
        if let Some(frame) = self.eval.frames.selected_frame().map(|frame| frame.id) {
            super::super::window_cmds::remember_selected_window_point_in_state(
                &mut self.eval.frames,
                &mut self.eval.buffers,
                frame,
            );
        }
        self.eval.gnu_run_pre_redisplay()?;
        if let Some(request) = self.eval.gnu_mini_geometry_request() {
            self.preparation = self.eval.redisplay_prepare_fn.take();
            if let Some(prepare) = self.preparation.as_mut() {
                // GNU's selected mini-only branch calls Lisp before the
                // ordinary mini-window current-buffer switch (xdisp.c:13313).
                // The producer reads request.buffer explicitly and returns
                // before row motion in this arm. Echo still owns its source.
                let mini_only = self
                    .eval
                    .frames
                    .get(request.frame)
                    .is_some_and(|frame| frame.root_window().id() == request.window);
                let result = if mini_only
                    && request.source == RedisplayMiniGeometrySource::ActiveMinibuffer
                {
                    prepare(self.eval, request)
                } else {
                    let mut source = MiniDisplaySource::enter(self.eval, request)?;
                    let result = prepare(source.eval, request);
                    source.finish(result)
                };
                result?;
                self.eval.gnu_redisplay_hooks.echo_geometry_window = match request.source {
                    RedisplayMiniGeometrySource::EchoArea if self.eval.has_current_message() => {
                        Some(request.window)
                    }
                    _ => None,
                };
            } else if self
                .eval
                .frames
                .get(request.frame)
                .is_some_and(|frame| frame.root_window().id() == request.window)
            {
                // GNU's mini-only branch calls Lisp before window change
                // hooks and never needs the ordinary minibuffer row walk.
                // Echo owns a temporary source; the selected-mini branch
                // leaves current-buffer untouched, as xdisp.c:13313 does.
                let result = if request.source == RedisplayMiniGeometrySource::EchoArea {
                    let mut source = MiniDisplaySource::enter(self.eval, request)?;
                    let result = source
                        .eval
                        .gnu_prepare_mini_only_without_renderer(request.frame, request.window);
                    source.finish(result)
                } else {
                    self.eval
                        .gnu_prepare_mini_only_without_renderer(request.frame, request.window)
                };
                result?;
                self.eval.gnu_redisplay_hooks.echo_geometry_window = match request.source {
                    RedisplayMiniGeometrySource::EchoArea if self.eval.has_current_message() => {
                        Some(request.window)
                    }
                    _ => None,
                };
            }
        }
        super::super::builtins::run_redisplay_window_change_hooks(self.eval)?;
        self.eval.gnu_distribute_text_marks();
        let unchanged =
            self.eval.last_redisplay_signature.as_ref() == Some(&self.eval.redisplay_signature());
        if (!force || crate::emacs_core::xdisp::redisplay_idle_skip_enabled())
            && !self.eval.echo_area_resize_exact_pending
            && unchanged
            && !(force && self.eval.displayed_buffer_changes_unacknowledged())
            && self.eval.gnu_redisplay_hooks.redisplay_windows.is_empty()
            && self.eval.gnu_redisplay_hooks.redisplay_frames.is_empty()
            && self.eval.gnu_redisplay_hooks.mode_line_windows.is_empty()
            && self.eval.gnu_redisplay_hooks.windows == PendingScope::None
            && self.eval.gnu_redisplay_hooks.mode_lines == PendingScope::None
        {
            return Ok(Value::NIL);
        }
        crate::emacs_core::hscroll::update_auto_hscroll_before_redisplay(self.eval);
        self.eval.gnu_distribute_text_marks();
        let accepted_before = self.eval.gnu_redisplay_hooks.accepted_serial;
        self.callback = self.eval.redisplay_fn.take();
        if let Some(callback) = self.callback.as_mut() {
            callback(self.eval);
        }
        // Main's mode-line safe evaluator transfers non-local exits through
        // its Context-owned slot. Consume it at the same restored transaction
        // boundary as hook failures, before acknowledging a successful frame.
        if let Some(flow) = self.eval.take_mode_line_display_flow() {
            return Err(flow);
        }
        if let Some(flow) = self.eval.gnu_redisplay_hooks.layout_flow.take() {
            return Err(flow);
        }
        if self.eval.gnu_redisplay_hooks.accepted_serial != accepted_before {
            self.eval.echo_area_resize_exact_pending = false;
            self.eval.gnu_redisplay_hooks.windows = PendingScope::None;
            self.eval.gnu_redisplay_hooks.mode_lines = PendingScope::None;
            self.eval.last_redisplay_signature = Some(self.eval.redisplay_signature());
        }
        Ok(Value::NIL)
    }
}

impl Drop for RedisplayTransaction<'_> {
    fn drop(&mut self) {
        if let Some(callback) = self.callback.take() {
            self.eval.redisplay_fn = Some(callback);
        }
        if let Some(preparation) = self.preparation.take() {
            self.eval.redisplay_prepare_fn = Some(preparation);
        }
        if let Some(restrictions) = self.restrictions.take() {
            self.eval
                .buffers
                .restore_outermost_restrictions(restrictions);
        }
        self.eval.gnu_redisplay_hooks.active = RedisplayActiveOwner::Idle;
        // Recovery only: normal Lisp binding exits propagate their Flow.
        self.eval.unbind_to(self.binding_count);
    }
}

#[cfg(test)]
#[path = "tests/redisplay_hook_ownership.rs"]
mod tests;

#[cfg(test)]
#[path = "redisplay_hooks/tests/mode_line_flow.rs"]
mod mode_line_flow_tests;

/// Temporary echo source, matching with_echo_area_buffer's owned unwind.
/// The old marker Values are explicitly rooted before temporarily replacing
/// the window's marker owner. Only buffer/point/start are restored; geometry
/// changes and arbitrary callback window properties remain live.
/// The guard exclusively borrows its owning Context and root stack; independent
/// mutators have disjoint saved source and unwind state.
struct MiniDisplaySource<'a> {
    eval: &'a mut Context,
    request: RedisplayMiniGeometryRequest,
    old_window: Option<(
        BufferId,
        LispCharPos1,
        LispCharPos1,
        LispCharPos1,
        crate::window::WindowPositionMarkerState,
        bool,
    )>,
    old_buffer: Option<BufferId>,
    old_windows_scope: PendingScope,
    deactivate_mark: Value,
    roots: Option<SpecpdlRootScopeState>,
    binding_count: usize,
    armed: bool,
}

impl<'a> MiniDisplaySource<'a> {
    fn enter(eval: &'a mut Context, request: RedisplayMiniGeometryRequest) -> Result<Self, Flow> {
        use crate::gc_trace::GcTrace;
        let roots = eval.save_specpdl_roots();
        let old_buffer = eval.buffers.current_buffer_id();
        let old_windows_scope = eval.gnu_redisplay_hooks.windows;
        let deactivate_mark = eval.eval_symbol("deactivate-mark").unwrap_or(Value::NIL);
        eval.push_specpdl_root(deactivate_mark);
        let old_window = if request.source == RedisplayMiniGeometrySource::EchoArea {
            let snapshot = eval
                .frames
                .get(request.frame)
                .and_then(|frame| frame.find_window(request.window))
                .cloned();
            if let Some(snapshot) = &snapshot {
                let mut saved_roots = Vec::new();
                snapshot.trace_roots(&mut saved_roots);
                for root in saved_roots {
                    eval.push_specpdl_root(root);
                }
            }
            match snapshot {
                Some(Window::Leaf {
                    buffer_id,
                    window_start,
                    point,
                    old_point,
                    position_markers,
                    force_start,
                    ..
                }) => Some((
                    buffer_id,
                    window_start,
                    point,
                    old_point,
                    position_markers,
                    force_start,
                )),
                _ => None,
            }
        } else {
            None
        };
        let binding_count = eval.specpdl.len();
        let mut scope = Self {
            eval,
            request,
            old_window,
            old_buffer,
            old_windows_scope,
            deactivate_mark,
            roots: Some(roots),
            binding_count,
            armed: true,
        };
        scope.eval.set_current_buffer_unrecorded(request.buffer)?;
        if scope.old_window.is_some() {
            if let Some(window) = scope
                .eval
                .frames
                .get_mut(request.frame)
                .and_then(|frame| frame.find_window_mut(request.window))
            {
                if let Window::Leaf {
                    buffer_id,
                    window_start,
                    point,
                    old_point,
                    position_markers,
                    ..
                } = window
                {
                    *buffer_id = request.buffer;
                    *window_start = LispCharPos1::ONE;
                    *point = LispCharPos1::ONE;
                    *old_point = LispCharPos1::ONE;
                    // Preserve old marker registrations until unwind. The
                    // saved window snapshot roots those marker objects.
                    *position_markers = crate::window::WindowPositionMarkerState::Detached;
                }
                crate::window::window_markers::attach_window_position_markers(
                    &mut scope.eval.buffers,
                    window,
                );
            }
            scope.eval.try_specbind_or_unwind_to(
                binding_count,
                intern("inhibit-read-only"),
                Value::T,
            )?;
            scope.eval.try_specbind_or_unwind_to(
                binding_count,
                intern("inhibit-modification-hooks"),
                Value::T,
            )?;
        }
        Ok(scope)
    }

    fn restore_source(&mut self) {
        if let Some((buffer, start, point, old_point, markers, old_force_start)) =
            self.old_window.take()
        {
            if let Some(window) = self
                .eval
                .frames
                .get_mut(self.request.frame)
                .and_then(|frame| frame.find_window_mut(self.request.window))
            {
                crate::window::window_markers::unchain_window_markers(
                    &mut self.eval.buffers,
                    window,
                );
                if let Window::Leaf {
                    buffer_id,
                    window_start,
                    point: current_point,
                    old_point: current_old_point,
                    position_markers,
                    force_start,
                    ..
                } = window
                {
                    *buffer_id = buffer;
                    *window_start = start;
                    *current_point = point;
                    *current_old_point = old_point;
                    *position_markers = markers;
                    *force_start = old_force_start;
                }
                crate::window::window_markers::set_window_start_with_marker(
                    &mut self.eval.buffers,
                    window,
                    start,
                );
                crate::window::window_markers::set_window_point_with_marker(
                    &mut self.eval.buffers,
                    window,
                    point,
                );
                crate::window::window_markers::set_window_old_point_with_marker(
                    &mut self.eval.buffers,
                    window,
                    old_point,
                );
            }
            self.eval.gnu_redisplay_hooks.windows = self.old_windows_scope;
            self.eval
                .obarray
                .set_symbol_value("deactivate-mark", self.deactivate_mark);
        }
        if let Some(buffer) = self.old_buffer.take() {
            self.eval.restore_current_buffer_if_live(buffer);
        }
    }

    fn finish(&mut self, result: EvalResult) -> EvalResult {
        let result = self.eval.unbind_to_with_result(self.binding_count, result);
        self.restore_source();
        if let Some(roots) = self.roots.take() {
            self.eval.restore_specpdl_roots(roots);
        }
        self.armed = false;
        result
    }
}
impl Drop for MiniDisplaySource<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.eval.unbind_to(self.binding_count);
            self.restore_source();
            if let Some(roots) = self.roots.take() {
                self.eval.restore_specpdl_roots(roots);
            }
        }
    }
}

/// A committed-start callback may be reached outside the normal redisplay
/// entry. The same private Context guard owns reentry there; its Lisp
/// inhibit-redisplay value remains untouched and Drop restores outer ownership.
/// Its exclusive Context borrow keeps saved ownership local to one mutator;
/// independent contexts never share this mutable guard.
pub(crate) struct CommittedScrollGuard<'a> {
    pub(crate) eval: &'a mut Context,
    previous: RedisplayActiveOwner,
}
impl Drop for CommittedScrollGuard<'_> {
    fn drop(&mut self) {
        self.eval.gnu_redisplay_hooks.active = self.previous;
    }
}

#[cfg(test)]
#[path = "tests/redisplay_mini_only.rs"]
mod mini_only_tests;

#[cfg(test)]
#[path = "tests/redisplay_mini_preparer_context.rs"]
mod mini_preparer_context_tests;

#[cfg(test)]
#[path = "tests/redisplay_mini_source_eligibility.rs"]
mod mini_source_eligibility_tests;

#[cfg(test)]
#[path = "tests/redisplay_transaction_owner.rs"]
mod transaction_owner_tests;
