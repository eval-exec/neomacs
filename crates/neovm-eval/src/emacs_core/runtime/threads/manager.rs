//! Live registrations and access to object-owned logical state.

use super::mailbox::MailboxError;
use super::*;
use neovm_value::tagged::gc::with_tagged_heap;
use neovm_value::tagged::header::{CondVarObj, MutexObj, ThreadObj, VecLikeType};
use neovm_value::tagged::transport::SharedRootError;

impl ThreadManager {
    pub fn new() -> Self {
        let ids = ThreadIds::new(with_tagged_heap(|heap| heap.heap_identity()));
        let main = ids.main_thread();
        let value = with_tagged_heap(|heap| {
            heap.alloc_thread(ThreadPayload::new(
                main,
                ThreadNativeState::new(ThreadPhase::Active(RunState::Running), None),
                ThreadChildren::new(
                    neovm_value::heap_types::threading::ThreadName::UNNAMED,
                    Value::NIL,
                    Value::NIL,
                    None,
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                    Vec::new(),
                ),
            ))
        });
        let mut threads = HashMap::new();
        threads.insert(main, value);
        Self {
            last_error: None,
            threads,
            mailbox_reservation: layout::MailboxReservation::new(),
            before_issuer: layout::BeforeIssuerReservation::new(),
            ids,
            current_thread: CurrentThreadOrdinal::MAIN,
            tail: layout::TailReservation::new(),
        }
    }

    #[doc(hidden)]
    pub fn try_create_thread(
        &mut self,
        function: Value,
        name: ThreadName,
    ) -> Result<ThreadId, Flow> {
        let id = self.ids.issue_thread().map_err(identity_exhausted)?;
        let value = with_tagged_heap(|heap| {
            heap.alloc_thread(ThreadPayload::new(
                id,
                ThreadNativeState::new(ThreadPhase::New, None),
                ThreadChildren::new(
                    name.0,
                    function,
                    Value::NIL,
                    None,
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                    Value::NIL,
                    Vec::new(),
                ),
            ))
        });
        // Allocation does not collect. Publish before the next Lisp safe point.
        self.threads.insert(id, value);
        Ok(id)
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn create_thread(&mut self, function: Value, name: ThreadName) -> u64 {
        self.try_create_thread(function, name)
            .expect("test identity issuance")
            .native_key()
    }

    #[inline]
    pub fn any_thread_created(&self) -> bool {
        self.ids.any_thread_created()
    }

    pub fn current_thread_id(&self) -> u64 {
        self.current_thread.native_key()
    }

    #[doc(hidden)]
    pub fn live_value(&self, native: u64) -> Option<Value> {
        self.threads
            .get(&self.ids.thread_from_native_key(native)?)
            .copied()
    }

    #[doc(hidden)]
    pub fn thread_payload(&self, value: Value) -> Option<&ThreadPayload> {
        if value.veclike_type()? != VecLikeType::Thread {
            return None;
        }
        let ptr = value.as_veclike_ptr()?.cast::<ThreadObj>();
        // SAFETY: callers supply a live Lisp argument or a published live-map
        // value; the observed Thread tag fixes the layout. Serial World access
        // and the borrow of self exclude collection/yield while this view lives.
        let payload = unsafe { (*ptr).payload() };
        let issued = self.ids.thread_from_native_key(payload.id().native_key())?;
        (issued == payload.id()).then_some(payload)
    }

    #[doc(hidden)]
    pub fn get_thread(&self, native: u64) -> Option<&ThreadPayload> {
        self.thread_payload(self.live_value(native)?)
    }

    #[doc(hidden)]
    pub fn update_thread<R>(
        &mut self,
        native: u64,
        update: impl FnOnce(&mut ThreadChildren) -> R,
    ) -> Option<R> {
        self.update_thread_value(self.live_value(native)?, update)
    }

    #[doc(hidden)]
    pub fn update_thread_value<R>(
        &mut self,
        value: Value,
        update: impl FnOnce(&mut ThreadChildren) -> R,
    ) -> Option<R> {
        self.thread_payload(value)?;
        with_tagged_heap(|heap| {
            // SAFETY: value is a validated rooted Lisp argument or live-map
            // edge; the manager is exclusively borrowed under serial World
            // authority. Updates below never allocate Lisp, collect or yield.
            unsafe { heap.with_thread_payload_mut(value, update) }
        })
    }

    #[doc(hidden)]
    pub fn update_thread_native<R>(
        &mut self,
        native: u64,
        update: impl FnOnce(&mut ThreadNativeState) -> R,
    ) -> Option<R> {
        self.update_thread_native_value(self.live_value(native)?, update)
    }

    #[doc(hidden)]
    pub fn update_thread_native_value<R>(
        &mut self,
        value: Value,
        update: impl FnOnce(&mut ThreadNativeState) -> R,
    ) -> Option<R> {
        self.thread_payload(value)?;
        with_tagged_heap(|heap| {
            // SAFETY: the validated live-map edge or Lisp argument roots this
            // same-World object. The exclusive, non-yielding native view cannot
            // access its identity or any Lisp child and never reaches a safe point.
            unsafe { heap.with_thread_native_mut(value, update) }
        })
    }

    pub fn start_thread(&mut self, native: u64) {
        self.update_thread_native(native, |state| {
            *state.phase_mut() = ThreadPhase::Active(RunState::Running);
            *state.executing_mut() = false;
        });
    }

    /// Mark whether this thread's body is on the simulation's OS stack.
    ///
    /// A release point reached from a running worker must not mistake
    /// another already-running worker for a created one and re-enter it.
    #[doc(hidden)]
    pub fn set_thread_executing(&mut self, native: u64, executing: bool) {
        self.update_thread_native(native, |state| *state.executing_mut() = executing);
    }

    pub fn discard_unstarted_thread(&mut self, native: u64) {
        if let Some(id) = self.ids.thread_from_native_key(native) {
            self.threads.remove(&id);
        }
    }

    pub fn enter_thread(&mut self, native: u64) -> u64 {
        std::mem::replace(&mut self.current_thread, CurrentThreadOrdinal(native)).native_key()
    }

    pub fn restore_thread(&mut self, native: u64) {
        self.current_thread = CurrentThreadOrdinal(native);
    }

    pub fn finish_thread(&mut self, native: u64, result: Value) {
        self.update_thread(native, |children| *children.result_mut() = result);
        self.update_thread_native(native, |state| {
            *state.phase_mut() = ThreadPhase::Completing(ThreadOutcome::Returned)
        });
    }

    pub fn signal_thread(&mut self, native: u64, error: Value) {
        if let Some(thread) = self.live_value(native) {
            self.signal_thread_for_value(thread, error);
        }
    }

    #[doc(hidden)]
    pub fn signal_thread_for_value(&mut self, value: Value, error: Value) {
        if self.thread_payload(value).is_none() {
            return;
        }
        // GNU `thread_set_error` (src/thread.c:985-991) overwrites the pending
        // pair and leaves the thread's phase alone. `Fthread_signal`
        // (:993-1034) wakes only `wait_condvar` (mutex-lock, condition-wait,
        // thread-join) via `thread_signal_callback` (:974-982). A select wait
        // (`sleep-for`) is not woken. A finished thread stays dead.
        let signal = split_signal_binding_value(error);
        let wake = signal_wakes_blocked_wait(
            self.thread_payload(value)
                .and_then(|thread| thread.native().blocked_wait()),
        );
        self.update_thread_value(value, |children| {
            *children.last_error_mut() = Some(error);
            let (symbol, data) = signal.unwrap_or((Value::NIL, Value::NIL));
            *children.error_symbol_mut() = symbol;
            *children.error_data_mut() = data;
        });
        if wake {
            self.update_thread_native_value(value, |state| {
                *state.woken_mut() = true;
            });
        }
    }

    pub fn record_thread_internal_error(&mut self, native: u64, error: Value) {
        self.update_thread(native, |children| {
            *children.last_error_mut() = Some(error);
            *children.error_symbol_mut() = Value::NIL;
            *children.error_data_mut() = Value::NIL;
        });
        self.update_thread_native(native, |state| {
            *state.phase_mut() = ThreadPhase::Completing(ThreadOutcome::InternalError)
        });
    }

    pub fn pending_thread_signal(&self, native: u64) -> Option<Flow> {
        self.pending_thread_signal_for_value(self.live_value(native)?)
    }

    /// Whether `thread-signal` left an error for this thread to raise.
    #[doc(hidden)]
    pub fn has_pending_thread_signal(&self, native: u64) -> bool {
        self.get_thread(native)
            .is_some_and(|thread| !thread.children().error_symbol().is_nil())
    }

    #[doc(hidden)]
    pub fn pending_thread_signal_for_value(&self, value: Value) -> Option<Flow> {
        let thread = self.thread_payload(value)?;
        if thread.children().error_symbol().is_nil() {
            return None;
        }
        thread
            .children()
            .last_error()
            .and_then(signal_from_binding_value)
    }

    pub fn take_pending_thread_signal(&mut self, native: u64) -> Option<Flow> {
        let (symbol, data) = self.take_thread_error(native)?;
        Some(signal_with_data_id(symbol.as_bare_symbol_id()?, data))
    }

    pub fn record_last_error(&mut self, error: Value) {
        self.last_error = Some(error);
    }

    pub fn thread_alive_p(&self, native: u64) -> bool {
        self.get_thread(native).is_some_and(thread_is_live)
    }

    #[doc(hidden)]
    pub fn thread_alive_for_value(&self, value: Value) -> bool {
        self.thread_payload(value).is_some_and(thread_is_live)
    }

    /// Whether this thread was created but has not run its function yet.
    ///
    /// GNU `Fmake_thread` (src/thread.c:885-947) links the thread and returns
    /// while the creating thread still holds the global lock; `run_thread`
    /// (src/thread.c:757+) executes only after `acquire_global_lock`. The
    /// simulation runs that body at the caller's next release point instead of
    /// during construction, so `New`/`Active(Running)` means "created, not run".
    /// `run_thread` has passed its first `acquire_global_lock` (thread.c:805)
    /// and installed `handlerlist_sentinel` (:810-815).
    #[doc(hidden)]
    pub fn handlerlist_installed(&self, native: u64) -> bool {
        self.get_thread(native)
            .is_some_and(|thread| thread.native().handlerlist() == HandlerList::Installed)
    }

    #[doc(hidden)]
    pub fn mark_handlerlist_installed(&mut self, native: u64) {
        self.update_thread_native(native, |state| {
            *state.handlerlist_mut() = HandlerList::Installed;
        });
    }

    pub(crate) fn thread_is_pending_sim_run(&self, native: u64) -> bool {
        self.get_thread(native).is_some_and(|thread| {
            !thread.native().executing()
                && matches!(
                    thread.native().phase(),
                    ThreadPhase::New | ThreadPhase::Active(RunState::Running)
                )
        })
    }

    /// The function object this thread will run, while it is still pending.
    pub(crate) fn thread_function(&self, native: u64) -> Option<Value> {
        self.get_thread(native)
            .map(|thread| thread.children().function())
    }

    pub fn thread_name(&self, native: u64) -> Option<ThreadName> {
        self.get_thread(native)
            .map(|thread| ThreadName(thread.children().name()))
    }

    #[doc(hidden)]
    pub fn thread_name_for_value(&self, value: Value) -> Option<ThreadName> {
        self.thread_payload(value)
            .map(|thread| ThreadName(thread.children().name()))
    }

    pub fn is_thread(&self, native: u64) -> bool {
        self.live_value(native).is_some()
    }
    pub fn thread_handle(&self, native: u64) -> Option<Value> {
        self.live_value(native)
    }
    pub fn thread_id_from_handle(&self, value: &Value) -> Option<u64> {
        self.thread_payload(*value)
            .map(|thread| thread.id().native_key())
    }

    pub fn all_thread_ids(&self) -> Vec<u64> {
        self.threads
            .keys()
            .map(|id| id.native_key())
            .filter(|id| self.thread_alive_p(*id))
            .collect()
    }

    pub fn thread_result(&self, native: u64) -> Value {
        self.get_thread(native)
            .map_or(Value::NIL, |thread| thread.children().result())
    }

    #[doc(hidden)]
    pub fn thread_result_for_value(&self, value: Value) -> Value {
        self.thread_payload(value)
            .map_or(Value::NIL, |thread| thread.children().result())
    }

    pub fn join_thread(&mut self, native: u64) -> Option<Value> {
        self.join_thread_for_value(self.live_value(native)?)
    }

    #[doc(hidden)]
    pub fn join_thread_for_value(&mut self, value: Value) -> Option<Value> {
        let error = self.thread_payload(value)?.children().last_error();
        self.update_thread_native_value(value, |state| {
            *state.join_observation_mut() = JoinObservation::Observed;
        })?;
        // GNU thread.c:853-859 unlinks the worker at its own final native
        // access, independently of join. A join only observes the actual Lisp
        // object; it must not retire an exiting carrier's completion root.
        error
    }

    pub fn block_thread(&mut self, native: u64, blocker: Value, remaining: Value) {
        self.update_thread(native, |children| {
            *children.event_object_mut() = blocker;
            *children.blocked_remaining_forms_mut() = remaining;
        });
        self.update_thread_native(native, |state| {
            *state.phase_mut() = ThreadPhase::Active(RunState::Blocked);
            // A fresh wait has not fired yet. GNU's waiter is asleep in
            // `sys_cond_wait` until a release/notify/signal wakes it.
            *state.woken_mut() = false;
        });
    }

    /// Record the typed operation a suspended wait resumes with.
    #[doc(hidden)]
    pub fn set_blocked_wait(&mut self, native: u64, wait: Option<BlockedWait>) -> bool {
        self.update_thread_native(native, |state| *state.blocked_wait_mut() = wait)
            .is_some()
    }

    #[doc(hidden)]
    pub fn blocked_wait(&self, native: u64) -> Option<BlockedWait> {
        self.get_thread(native)?.native().blocked_wait()
    }

    #[doc(hidden)]
    pub fn thread_is_woken(&self, native: u64) -> bool {
        self.get_thread(native)
            .is_some_and(|thread| thread.native().woken())
    }

    /// Consume a wake that did not end the wait: another woken mutex waiter
    /// took the mutex first, so GNU's lock loop re-waits on the mutex
    /// condition (src/thread.c:228-241, :470).
    #[doc(hidden)]
    pub fn clear_thread_woken(&mut self, native: u64) {
        self.update_thread_native(native, |state| *state.woken_mut() = false);
    }

    /// Whether this registration is currently waiting on OBJECT.
    ///
    /// A thread is waiting when GNU's `event_object` names OBJECT and the
    /// thread is either suspended (a stored continuation) or inside its own
    /// typed wait (a condition wait releases the global lock before the wait
    /// is published as a continuation).
    fn is_waiting_on(&self, id: u64, object: &Value) -> bool {
        let Some(thread) = self.get_thread(id) else {
            return false;
        };
        if !eq_value(&thread.children().event_object(), object) {
            return false;
        }
        thread.native().phase() == ThreadPhase::Active(RunState::Blocked)
            || thread.native().blocked_wait().is_some()
    }

    fn mark_woken(&mut self, id: u64) {
        self.update_thread_native(id, |state| *state.woken_mut() = true);
    }

    /// Wake every thread waiting on BLOCKER. GNU `sys_cond_broadcast`:
    /// mutex release (`lisp_mutex_unlock`, thread.c:259-263) and
    /// `lisp_mutex_unlock_for_wait` (:276-288) use it, and a waiting
    /// thread's `event_object` keeps rooting the wait object until the
    /// waiter itself clears it (:346, :455, :465).
    pub fn wake_threads_blocked_on(&mut self, blocker: Value) {
        let ids = self.all_native_registrations();
        for id in ids {
            if id != 0 && self.is_waiting_on(id, &blocker) {
                self.mark_woken(id);
            }
        }
        // The main thread runs its waits in place; its wake is clearing the
        // wait object it published (GNU event_object stays until the waiter
        // clears it too).
        self.wake_main_waiting_on(blocker);
    }

    /// Wake at most one thread waiting on BLOCKER (GNU `sys_cond_signal`).
    /// Selection is by registration order so the simulation is deterministic.
    #[doc(hidden)]
    pub fn wake_one_thread_blocked_on(&mut self, blocker: Value) -> bool {
        let mut ids = self.all_native_registrations();
        ids.sort_unstable();
        for id in ids {
            if id == 0 {
                if self.main_waiting_on(&blocker) {
                    return self.wake_main_waiting_on(blocker);
                }
                continue;
            }
            if self.is_waiting_on(id, &blocker) {
                self.mark_woken(id);
                return true;
            }
        }
        false
    }

    /// Publish the object the main thread is waiting on while a blocking
    /// builtin waits in place (GNU `event_object`).
    #[doc(hidden)]
    pub fn set_main_wait_object(&mut self, object: Value) -> bool {
        self.set_thread_blocker(self.ids.main_thread().native_key(), object)
    }

    #[doc(hidden)]
    pub fn clear_main_wait_object(&mut self) -> bool {
        self.clear_thread_blocker(self.ids.main_thread().native_key())
    }

    #[doc(hidden)]
    pub fn main_waiting_on(&self, object: &Value) -> bool {
        self.thread_blocker(self.ids.main_thread().native_key())
            .is_some_and(|blocker| eq_value(&blocker, object))
    }

    /// End the main thread's in-place wait when a notification or release
    /// names its wait object.
    #[doc(hidden)]
    pub fn wake_main_waiting_on(&mut self, object: Value) -> bool {
        if !self.main_waiting_on(&object) {
            return false;
        }
        self.clear_main_wait_object()
    }

    pub fn thread_ready_to_resume(&self, native: u64) -> bool {
        self.get_thread(native).is_some_and(|thread| {
            thread.native().phase() == ThreadPhase::Active(RunState::Blocked)
                && (thread.native().woken()
                    || (thread.native().blocked_wait().is_none()
                        && thread.children().event_object().is_nil()))
        })
    }

    pub fn blocked_sleep_duration(&self, native: u64) -> Option<Duration> {
        let thread = self.get_thread(native)?;
        if thread.native().phase() != ThreadPhase::Active(RunState::Blocked) {
            return None;
        }
        match thread.native().blocked_wait() {
            Some(BlockedWait::Select { deadline }) => {
                Some(deadline.saturating_duration_since(std::time::Instant::now()))
            }
            Some(BlockedWait::MutexLock | BlockedWait::ConditionWait(_)) | None => None,
        }
    }

    pub fn wake_thread(&mut self, native: u64) {
        if self
            .get_thread(native)
            .is_some_and(|thread| thread.native().phase() == ThreadPhase::Active(RunState::Blocked))
        {
            self.mark_woken(native);
        }
    }

    pub fn take_blocked_remaining_forms(&mut self, native: u64) -> Option<Value> {
        if !self.thread_ready_to_resume(native) {
            return None;
        }
        let remaining = self.update_thread(native, |children| {
            std::mem::replace(children.blocked_remaining_forms_mut(), Value::NIL)
        })?;
        self.update_thread_native(native, |state| {
            *state.phase_mut() = ThreadPhase::Active(RunState::Running)
        });
        Some(remaining)
    }

    pub fn blocked_remaining_forms_if_ready(&self, native: u64) -> Option<Value> {
        if !self.thread_ready_to_resume(native) {
            return None;
        }
        self.get_thread(native)
            .map(|thread| thread.children().blocked_remaining_forms())
    }

    pub fn last_error(&mut self, cleanup: bool) -> Value {
        let value = self.last_error.unwrap_or(Value::NIL);
        if cleanup {
            self.last_error = None;
        }
        value
    }

    pub fn thread_buffer_disposition(&self, native: u64) -> Option<Value> {
        self.get_thread(native)
            .map(|t| t.children().buffer_disposition())
    }
    #[doc(hidden)]
    pub fn thread_buffer_disposition_for_value(&self, value: Value) -> Option<Value> {
        self.thread_payload(value)
            .map(|t| t.children().buffer_disposition())
    }
    pub fn set_thread_buffer_disposition(&mut self, native: u64, value: Value) -> bool {
        self.update_thread(native, |t| *t.buffer_disposition_mut() = value)
            .is_some()
    }
    #[doc(hidden)]
    pub fn set_thread_buffer_disposition_for_value(&mut self, thread: Value, value: Value) -> bool {
        self.update_thread_value(thread, |t| *t.buffer_disposition_mut() = value)
            .is_some()
    }
    pub fn thread_current_buffer(&self, native: u64) -> Option<neovm_edit::buffer::BufferId> {
        self.get_thread(native)
            .and_then(|t| t.native().current_buffer())
    }
    #[cfg(test)]
    #[doc(hidden)]
    pub fn thread_current_buffer_for_value(
        &self,
        thread: Value,
    ) -> Option<neovm_edit::buffer::BufferId> {
        self.thread_payload(thread)
            .and_then(|t| t.native.current_buffer)
    }
    pub fn set_thread_current_buffer(
        &mut self,
        native: u64,
        buffer: Option<neovm_edit::buffer::BufferId>,
    ) -> bool {
        let Some(thread) = self.get_thread(native) else {
            return false;
        };
        if thread.native().current_buffer() == buffer {
            return true;
        }
        self.update_thread_native(native, |state| *state.current_buffer_mut() = buffer)
            .is_some()
    }
    pub fn thread_blocker(&self, native: u64) -> Option<Value> {
        self.get_thread(native).map(|t| t.children().event_object())
    }
    #[doc(hidden)]
    pub fn thread_blocker_for_value(&self, value: Value) -> Option<Value> {
        self.thread_payload(value)
            .map(|t| t.children().event_object())
    }
    pub fn set_thread_blocker(&mut self, native: u64, blocker: Value) -> bool {
        self.update_thread(native, |t| *t.event_object_mut() = blocker)
            .is_some()
    }
    pub fn clear_thread_blocker(&mut self, native: u64) -> bool {
        self.set_thread_blocker(native, Value::NIL)
    }

    pub fn mark_thread_exited(&mut self, native: u64) {
        self.update_thread_native(native, |state| {
            let outcome = match state.phase() {
                ThreadPhase::New | ThreadPhase::Active(_) => ThreadOutcome::Returned,
                ThreadPhase::Completing(outcome) | ThreadPhase::Terminal(outcome) => outcome,
            };
            *state.phase_mut() = ThreadPhase::Terminal(outcome);
        });
    }

    pub fn set_thread_error(&mut self, native: u64, symbol: Value, data: Value) {
        if let Some(thread) = self.live_value(native) {
            self.set_thread_error_for_value(thread, symbol, data);
        }
    }

    #[doc(hidden)]
    pub fn set_thread_error_for_value(&mut self, value: Value, symbol: Value, data: Value) {
        let error = Value::cons(symbol, data);
        self.update_thread_value(value, |thread| {
            *thread.error_symbol_mut() = symbol;
            *thread.error_data_mut() = data;
            *thread.last_error_mut() = Some(error);
        });
    }

    pub fn take_thread_error(&mut self, native: u64) -> Option<(Value, Value)> {
        self.update_thread(native, |thread| {
            if thread.error_symbol().is_nil() {
                return None;
            }
            Some((
                std::mem::replace(thread.error_symbol_mut(), Value::NIL),
                std::mem::replace(thread.error_data_mut(), Value::NIL),
            ))
        })?
    }

    pub fn restore_thread_error(&mut self, native: u64, symbol: Value, data: Value) {
        self.update_thread(native, |thread| {
            if thread.error_symbol().is_nil() {
                *thread.error_symbol_mut() = symbol;
                *thread.error_data_mut() = data;
            }
        });
    }

    #[doc(hidden)]
    pub fn all_native_registrations(&self) -> Vec<u64> {
        self.threads.keys().map(|id| id.native_key()).collect()
    }

    pub fn buffer_held_by_other_thread(&self, buffer: neovm_edit::buffer::BufferId) -> bool {
        self.threads.iter().any(|(id, value)| {
            self.thread_payload(*value).is_some_and(|thread| {
                id.native_key() != self.current_thread.native_key()
                    && thread_is_running(thread)
                    && thread.native().current_buffer() == Some(buffer)
                    && thread.children().buffer_disposition().is_t()
            })
        })
    }

    pub fn buffer_current_in_other_thread(&self, buffer: neovm_edit::buffer::BufferId) -> bool {
        self.threads.iter().any(|(id, value)| {
            self.thread_payload(*value).is_some_and(|thread| {
                id.native_key() != self.current_thread.native_key()
                    && thread_is_running(thread)
                    && thread.native().current_buffer() == Some(buffer)
            })
        })
    }

    pub fn move_threads_off_killed_buffer(
        &mut self,
        buffer: neovm_edit::buffer::BufferId,
        other: neovm_edit::buffer::BufferId,
    ) {
        for native in self.all_native_registrations() {
            let Some(thread) = self.get_thread(native) else {
                continue;
            };
            if native == self.current_thread.native_key()
                || !thread_is_running(thread)
                || thread.native().current_buffer() != Some(buffer)
            {
                continue;
            }
            let pending = if thread
                .children()
                .buffer_disposition()
                .is_bare_symbol_named("silently")
            {
                None
            } else {
                let symbol = Value::symbol("thread-buffer-killed");
                Some((symbol, Value::cons(symbol, Value::NIL)))
            };
            if let Some((symbol, error)) = pending {
                self.update_thread(native, |children| {
                    *children.error_symbol_mut() = symbol;
                    *children.error_data_mut() = Value::NIL;
                    *children.last_error_mut() = Some(error);
                });
            }
            self.set_thread_current_buffer(native, Some(other));
        }
    }

    #[doc(hidden)]
    pub fn create_mutex(&mut self, name: Option<LispString>) -> Result<MutexHandle, Flow> {
        let id = self.ids.issue_mutex().map_err(identity_exhausted)?;
        let value = with_tagged_heap(|heap| heap.alloc_mutex(MutexPayload::new(id, name)));
        Ok(MutexHandle { value, id })
    }

    #[doc(hidden)]
    pub fn mutex_payload(&self, mutex: MutexHandle) -> Option<&MutexPayload> {
        let ptr = mutex.value.as_veclike_ptr()?.cast::<MutexObj>();
        // SAFETY: private handle constructors validate the rooted object's
        // exact tag and issued same-World identity. A borrowed view cannot cross
        // a Context safe point; no ownership edge to the native owner is added.
        Some(unsafe { (*ptr).payload() })
    }

    #[doc(hidden)]
    pub fn update_mutex<R>(
        &mut self,
        mutex: MutexHandle,
        update: impl FnOnce(&mut MutexPayload) -> R,
    ) -> Option<R> {
        with_tagged_heap(|heap| {
            // SAFETY: typed handle is rooted by its Lisp argument/continuation,
            // World serial authority excludes mutation and workers defer the
            // payload; callers below do not allocate Lisp, collect or yield.
            unsafe { heap.with_mutex_payload_mut(mutex.value, update) }
        })
    }

    #[doc(hidden)]
    pub fn mutex_id_from_handle(&self, value: &Value) -> Option<MutexHandle> {
        if value.veclike_type()? != VecLikeType::Mutex {
            return None;
        }
        let ptr = value.as_veclike_ptr()?.cast::<MutexObj>();
        // SAFETY: observed live Lisp Mutex value fixes the concrete layout.
        let id = unsafe { (*ptr).payload().id() };
        (self.ids.mutex_from_native_key(id.native_key())? == id)
            .then_some(MutexHandle { value: *value, id })
    }

    #[doc(hidden)]
    pub fn is_mutex(&self, mutex: MutexHandle) -> bool {
        self.mutex_id_from_handle(&mutex.value).is_some()
    }
    #[doc(hidden)]
    pub fn mutex_handle(&self, mutex: MutexHandle) -> Value {
        mutex.value
    }
    #[doc(hidden)]
    pub fn mutex_name(&self, mutex: MutexHandle) -> Option<&LispString> {
        self.mutex_payload(mutex)?.name().as_ref()
    }
    #[doc(hidden)]
    pub fn mutex_free(&self, mutex: MutexHandle) -> bool {
        self.mutex_payload(mutex)
            .is_some_and(|payload| payload.ownership() == MutexOwnership::Unlocked)
    }
    #[doc(hidden)]
    pub fn mutex_owned_by_current_thread(&self, mutex: MutexHandle) -> bool {
        self.mutex_payload(mutex)
            .is_some_and(|payload| match payload.ownership() {
                MutexOwnership::Unlocked => false,
                MutexOwnership::Locked { owner, depth: _ } => {
                    owner.native_key() == self.current_thread.native_key()
                }
            })
    }

    #[doc(hidden)]
    pub fn mutex_lock(&mut self, mutex: MutexHandle) -> MutexAcquisition {
        let Some(current) = self
            .ids
            .thread_from_native_key(self.current_thread.native_key())
        else {
            return MutexAcquisition::Blocked;
        };
        self.update_mutex(mutex, |state| match state.ownership() {
            MutexOwnership::Unlocked => {
                *state.ownership_mut() = MutexOwnership::Locked {
                    owner: current,
                    depth: RecursionDepth::ONE,
                };
                MutexAcquisition::Acquired
            }
            MutexOwnership::Locked { owner, depth } if owner == current => {
                let Some(depth) = depth.checked_increment() else {
                    return MutexAcquisition::DepthExhausted;
                };
                *state.ownership_mut() = MutexOwnership::Locked { owner, depth };
                MutexAcquisition::Acquired
            }
            MutexOwnership::Locked { .. } => MutexAcquisition::Blocked,
        })
        .unwrap_or(MutexAcquisition::Blocked)
    }

    #[doc(hidden)]
    pub fn mutex_unlock(&mut self, mutex: MutexHandle) -> MutexRelease {
        let current = self.current_thread.native_key();
        self.update_mutex(mutex, |state| match state.ownership() {
            MutexOwnership::Unlocked => MutexRelease::NotOwner,
            MutexOwnership::Locked { owner, depth } if owner.native_key() == current => {
                match RecursionDepth::new(depth.get() - 1) {
                    Some(depth) => {
                        *state.ownership_mut() = MutexOwnership::Locked { owner, depth };
                        MutexRelease::StillLocked
                    }
                    None => {
                        *state.ownership_mut() = MutexOwnership::Unlocked;
                        MutexRelease::Released
                    }
                }
            }
            MutexOwnership::Locked { .. } => MutexRelease::NotOwner,
        })
        .unwrap_or(MutexRelease::NotOwner)
    }

    #[doc(hidden)]
    pub fn mutex_unlock_for_wait(&mut self, mutex: MutexHandle) -> Option<RecursionDepth> {
        let current = self.current_thread.native_key();
        self.update_mutex(mutex, |state| match state.ownership() {
            MutexOwnership::Unlocked => None,
            MutexOwnership::Locked { owner, depth } if owner.native_key() == current => {
                *state.ownership_mut() = MutexOwnership::Unlocked;
                Some(depth)
            }
            MutexOwnership::Locked { .. } => None,
        })?
    }

    #[doc(hidden)]
    pub fn mutex_relock_after_wait(
        &mut self,
        mutex: MutexHandle,
        depth: RecursionDepth,
    ) -> MutexAcquisition {
        let Some(owner) = self
            .ids
            .thread_from_native_key(self.current_thread.native_key())
        else {
            return MutexAcquisition::Blocked;
        };
        self.update_mutex(mutex, |state| match state.ownership() {
            MutexOwnership::Unlocked => {
                *state.ownership_mut() = MutexOwnership::Locked { owner, depth };
                MutexAcquisition::Acquired
            }
            MutexOwnership::Locked { .. } => MutexAcquisition::Blocked,
        })
        .unwrap_or(MutexAcquisition::Blocked)
    }

    #[doc(hidden)]
    pub fn create_condition_variable(
        &mut self,
        mutex: MutexHandle,
        name: Option<LispString>,
    ) -> Result<CondVarHandle, Flow> {
        let id = self.ids.issue_condvar().map_err(identity_exhausted)?;
        let ptr = mutex
            .value
            .as_veclike_ptr()
            .ok_or_else(invalid_mutex)?
            .cast::<MutexObj>();
        // SAFETY: mutex is a rooted, validated typed handle and no safepoint
        // occurs before the new condition owns its strong association edge.
        let association =
            MutexReference::from_mutex(mutex.value, unsafe { &*ptr }).ok_or_else(invalid_mutex)?;
        let payload = CondVarPayload::new(id, name, association).map_err(|_| invalid_mutex())?;
        let value = with_tagged_heap(|heap| heap.alloc_condvar(payload));
        Ok(CondVarHandle { value, id, mutex })
    }

    #[doc(hidden)]
    pub fn condition_payload(&self, condition: CondVarHandle) -> Option<&CondVarPayload> {
        let ptr = condition.value.as_veclike_ptr()?.cast::<CondVarObj>();
        // SAFETY: private handle construction fixes the rooted live layout;
        // the serial borrow excludes payload mutation/collection during view.
        Some(unsafe { (*ptr).payload() })
    }

    #[doc(hidden)]
    pub fn condition_variable_id_from_handle(&self, value: &Value) -> Option<CondVarHandle> {
        if value.veclike_type()? != VecLikeType::CondVar {
            return None;
        }
        let ptr = value.as_veclike_ptr()?.cast::<CondVarObj>();
        // SAFETY: observed live Lisp CondVar value fixes the concrete layout.
        let payload = unsafe { (*ptr).payload() };
        let id = payload.id();
        let association = payload.association();
        let mutex = MutexHandle {
            value: association.value(),
            id: association.id(),
        };
        (self.ids.condvar_from_native_key(id.native_key())? == id).then_some(CondVarHandle {
            value: *value,
            id,
            mutex,
        })
    }

    #[doc(hidden)]
    pub fn is_condition_variable(&self, condition: CondVarHandle) -> bool {
        self.condition_variable_id_from_handle(&condition.value)
            .is_some()
    }
    #[doc(hidden)]
    pub fn condition_variable_handle(&self, condition: CondVarHandle) -> Value {
        condition.value
    }
    #[doc(hidden)]
    pub fn condition_variable_name(&self, condition: CondVarHandle) -> Option<&LispString> {
        self.condition_payload(condition)?.name().as_ref()
    }
    #[doc(hidden)]
    pub fn condition_variable_mutex(&self, condition: CondVarHandle) -> MutexHandle {
        condition.mutex
    }
}

fn identity_exhausted(error: neovm_value::heap_types::threading::NativeIdExhausted) -> Flow {
    signal("error", vec![Value::string(error.to_string())])
}

fn invalid_mutex() -> Flow {
    signal("error", vec![Value::string("Invalid mutex association")])
}

fn thread_is_running(thread: &ThreadPayload) -> bool {
    match thread.native().phase() {
        ThreadPhase::New | ThreadPhase::Active(_) => true,
        ThreadPhase::Completing(_) | ThreadPhase::Terminal(_) => false,
    }
}

fn signal_wakes_blocked_wait(wait: Option<BlockedWait>) -> bool {
    // Exhaustive so a future select-class wait cannot be woken by accident.
    // GNU broadcasts only `wait_condvar` (src/thread.c:974-982, :1029-1030).
    match wait {
        Some(BlockedWait::MutexLock | BlockedWait::ConditionWait(_)) => true,
        // `thread_select` is not `wait_condvar` (src/thread.c:974-982).
        Some(BlockedWait::Select { .. }) | None => false,
    }
}

fn thread_is_live(thread: &ThreadPayload) -> bool {
    // GNU `thread_live_p` is `(STATE)->m_specpdl != NULL` (src/thread.c:76).
    // `run_thread` clears that pointer at :822-823, before join, and then
    // unlinks the thread (:853-859). A finished thread is dead either way.
    thread_is_running(thread)
}

impl ThreadManager {
    /// Refusal preserves both the target and the caller's temporary leases.
    #[doc(hidden)]
    pub fn queue_thread_event(
        &mut self,
        target: ThreadId,
        event: &InFlightThreadEvent,
    ) -> Result<(), MailboxError> {
        let value = self
            .threads
            .get(&target)
            .copied()
            .ok_or(MailboxError::MissingTarget)?;
        with_tagged_heap(|heap| {
            if target.heap_identity() != heap.heap_identity() {
                return Err(SharedRootError::ForeignHeap {
                    owner: target.heap_identity(),
                    mutator: heap.heap_identity(),
                }
                .into());
            }
            let stored = StoredThreadEvent::try_from_roots(
                heap,
                &[
                    event.sender.clone(),
                    event.error_symbol.clone(),
                    event.data.clone(),
                ],
            )?;
            // SAFETY: the target is rooted by the same-heap live registration;
            // the validated temporary leases keep every input live until the
            // canonical child wrapper publishes and accounts the new edge.
            unsafe {
                heap.with_thread_payload_mut(value, |children| {
                    children.stored_events_mut().push(stored)
                })
            }
            .ok_or(MailboxError::MissingTarget)
        })?;
        if target == self.ids.main_thread() {
            self.mailbox_reservation.main = layout::MainMailboxState::Pending;
        }
        Ok(())
    }

    /// Build ALL receiving leases before removing any stored child. No Lisp
    /// allocation, yield or collection intervenes in this same-World transfer.
    #[doc(hidden)]
    pub fn take_thread_events(
        &mut self,
        target: ThreadId,
    ) -> Result<Vec<InFlightThreadEvent>, MailboxError> {
        if target == self.ids.main_thread() {
            match self.mailbox_reservation.main {
                layout::MainMailboxState::Empty => return Ok(Vec::new()),
                layout::MainMailboxState::Pending => {}
            }
        }
        let value = self
            .threads
            .get(&target)
            .copied()
            .ok_or(MailboxError::MissingTarget)?;
        let payload = self
            .thread_payload(value)
            .ok_or(MailboxError::MissingTarget)?;
        if payload.children().stored_events().is_empty() {
            return Ok(Vec::new());
        }
        let receiving = with_tagged_heap(|heap| {
            if target.heap_identity() != heap.heap_identity() {
                return Err(SharedRootError::ForeignHeap {
                    owner: target.heap_identity(),
                    mutator: heap.heap_identity(),
                });
            }
            payload
                .children()
                .stored_events()
                .iter()
                .map(|event| {
                    let [sender, error_symbol, data] = event.values();
                    // SAFETY: these words are the validated target's ordinary
                    // traced children. The live registration roots it, and its
                    // immutable borrow prevents removal until all receiving roots
                    // are registered. No raw caller-supplied Value is admitted.
                    let [sender, error_symbol, data] = unsafe {
                        [sender, error_symbol, data].map(|value| SharedRoot::new(heap, value))
                    };
                    Ok(InFlightThreadEvent {
                        sender,
                        error_symbol,
                        data,
                    })
                })
                .collect::<Result<Vec<_>, SharedRootError>>()
        })?;
        // The receiving leases now own all three words of every event. The
        // canonical wrapper records old edges before clearing the collection.
        self.update_thread_value(value, |children| children.stored_events_mut().clear())
            .ok_or(MailboxError::MissingTarget)?;
        if target == self.ids.main_thread() {
            self.mailbox_reservation.main = layout::MainMailboxState::Empty;
        }
        Ok(receiving)
    }
}
