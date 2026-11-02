//! Live registrations and access to object-owned logical state.

use super::mailbox::MailboxError;
use super::*;
use crate::tagged::gc::with_tagged_heap;
use crate::tagged::header::{CondVarObj, MutexObj, ThreadObj, VecLikeType};
use crate::tagged::transport::SharedRootError;

impl ThreadManager {
    pub fn new() -> Self {
        let ids = ThreadIds::new(with_tagged_heap(|heap| heap.heap_identity()));
        let main = ids.main_thread();
        let value = with_tagged_heap(|heap| {
            heap.alloc_thread(ThreadPayload::new(
                main,
                ThreadNativeState::new(ThreadPhase::Active(RunState::Running), None),
                ThreadChildren {
                    name: crate::heap_types::threading::ThreadName::UNNAMED,
                    function: Value::NIL,
                    result: Value::NIL,
                    last_error: None,
                    buffer_disposition: Value::NIL,
                    event_object: Value::NIL,
                    error_symbol: Value::NIL,
                    error_data: Value::NIL,
                    blocked_remaining_forms: Value::NIL,
                    stored_events: Vec::new(),
                },
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

    pub(crate) fn try_create_thread(
        &mut self,
        function: Value,
        name: ThreadName,
    ) -> Result<ThreadId, Flow> {
        let id = self.ids.issue_thread().map_err(identity_exhausted)?;
        let value = with_tagged_heap(|heap| {
            heap.alloc_thread(ThreadPayload::new(
                id,
                ThreadNativeState::new(ThreadPhase::New, None),
                ThreadChildren {
                    name: name.0,
                    function,
                    result: Value::NIL,
                    last_error: None,
                    buffer_disposition: Value::NIL,
                    event_object: Value::NIL,
                    error_symbol: Value::NIL,
                    error_data: Value::NIL,
                    blocked_remaining_forms: Value::NIL,
                    stored_events: Vec::new(),
                },
            ))
        });
        // Allocation does not collect. Publish before the next Lisp safe point.
        self.threads.insert(id, value);
        Ok(id)
    }

    #[cfg(test)]
    pub(crate) fn create_thread(&mut self, function: Value, name: ThreadName) -> u64 {
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

    fn live_value(&self, native: u64) -> Option<Value> {
        self.threads
            .get(&self.ids.thread_from_native_key(native)?)
            .copied()
    }

    fn thread_payload(&self, value: Value) -> Option<&ThreadPayload> {
        if value.veclike_type()? != VecLikeType::Thread {
            return None;
        }
        let ptr = value.as_veclike_ptr()?.cast::<ThreadObj>();
        // SAFETY: callers supply a live Lisp argument or a published live-map
        // value; the observed Thread tag fixes the layout. Serial World access
        // and the borrow of self exclude collection/yield while this view lives.
        let payload = unsafe { &(*ptr).payload };
        let issued = self.ids.thread_from_native_key(payload.id().native_key())?;
        (issued == payload.id()).then_some(payload)
    }

    pub(crate) fn get_thread(&self, native: u64) -> Option<&ThreadPayload> {
        self.thread_payload(self.live_value(native)?)
    }

    fn update_thread<R>(
        &mut self,
        native: u64,
        update: impl FnOnce(&mut ThreadChildren) -> R,
    ) -> Option<R> {
        self.update_thread_value(self.live_value(native)?, update)
    }

    fn update_thread_value<R>(
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

    fn update_thread_native<R>(
        &mut self,
        native: u64,
        update: impl FnOnce(&mut ThreadNativeState) -> R,
    ) -> Option<R> {
        self.update_thread_native_value(self.live_value(native)?, update)
    }

    fn update_thread_native_value<R>(
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
            state.phase = ThreadPhase::Active(RunState::Running)
        });
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
        self.update_thread(native, |children| children.result = result);
        self.update_thread_native(native, |state| {
            state.phase = ThreadPhase::Completing(ThreadOutcome::Returned)
        });
    }

    pub fn signal_thread(&mut self, native: u64, error: Value) {
        if let Some(thread) = self.live_value(native) {
            self.signal_thread_for_value(thread, error);
        }
    }

    pub(crate) fn signal_thread_for_value(&mut self, value: Value, error: Value) {
        let Some(phase) = self.thread_payload(value).map(|thread| thread.native.phase) else {
            return;
        };
        let signal = split_signal_binding_value(error);
        self.update_thread_value(value, |children| {
            children.last_error = Some(error);
            let (symbol, data) = signal.unwrap_or((Value::NIL, Value::NIL));
            children.error_symbol = symbol;
            children.error_data = data;
            if phase == ThreadPhase::Active(RunState::Blocked) {
                children.event_object = Value::NIL;
            }
        });
        self.update_thread_native_value(value, |state| {
            state.phase = match phase {
                ThreadPhase::Active(RunState::Blocked) => phase,
                ThreadPhase::New
                | ThreadPhase::Active(RunState::Running)
                | ThreadPhase::Completing(_) => ThreadPhase::Completing(ThreadOutcome::Signaled),
                ThreadPhase::Terminal(_) => ThreadPhase::Terminal(ThreadOutcome::Signaled),
            };
        });
    }

    pub fn record_thread_internal_error(&mut self, native: u64, error: Value) {
        self.update_thread(native, |children| {
            children.last_error = Some(error);
            children.error_symbol = Value::NIL;
            children.error_data = Value::NIL;
        });
        self.update_thread_native(native, |state| {
            state.phase = ThreadPhase::Completing(ThreadOutcome::InternalError)
        });
    }

    pub fn pending_thread_signal(&self, native: u64) -> Option<Flow> {
        self.pending_thread_signal_for_value(self.live_value(native)?)
    }

    pub(crate) fn pending_thread_signal_for_value(&self, value: Value) -> Option<Flow> {
        let thread = self.thread_payload(value)?;
        if thread.children.error_symbol.is_nil() {
            return None;
        }
        thread
            .children
            .last_error
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

    pub(crate) fn thread_alive_for_value(&self, value: Value) -> bool {
        self.thread_payload(value).is_some_and(thread_is_live)
    }

    pub fn thread_name(&self, native: u64) -> Option<ThreadName> {
        self.get_thread(native)
            .map(|thread| ThreadName(thread.children.name))
    }

    pub(crate) fn thread_name_for_value(&self, value: Value) -> Option<ThreadName> {
        self.thread_payload(value)
            .map(|thread| ThreadName(thread.children.name))
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
            .map_or(Value::NIL, |thread| thread.children.result)
    }

    pub(crate) fn thread_result_for_value(&self, value: Value) -> Value {
        self.thread_payload(value)
            .map_or(Value::NIL, |thread| thread.children.result)
    }

    pub fn join_thread(&mut self, native: u64) -> Option<Value> {
        self.join_thread_for_value(self.live_value(native)?)
    }

    pub(crate) fn join_thread_for_value(&mut self, value: Value) -> Option<Value> {
        let error = self.thread_payload(value)?.children.last_error;
        self.update_thread_native_value(value, |state| {
            state.join_observation = JoinObservation::Observed;
        })?;
        // GNU thread.c:853-859 unlinks the worker at its own final native
        // access, independently of join. A join only observes the actual Lisp
        // object; it must not retire an exiting carrier's completion root.
        error
    }

    pub fn block_thread(&mut self, native: u64, blocker: Value, remaining: Value) {
        self.update_thread(native, |children| {
            children.event_object = blocker;
            children.blocked_remaining_forms = remaining;
        });
        self.update_thread_native(native, |state| {
            state.phase = ThreadPhase::Active(RunState::Blocked)
        });
    }

    pub fn wake_threads_blocked_on(&mut self, blocker: Value) {
        for id in self.all_native_registrations() {
            if self.get_thread(id).is_some_and(|thread| {
                thread.native.phase == ThreadPhase::Active(RunState::Blocked)
                    && eq_value(&thread.children.event_object, &blocker)
            }) {
                self.update_thread(id, |children| children.event_object = Value::NIL);
            }
        }
    }

    pub fn thread_ready_to_resume(&self, native: u64) -> bool {
        self.get_thread(native).is_some_and(|thread| {
            thread.native.phase == ThreadPhase::Active(RunState::Blocked)
                && thread.children.event_object.is_nil()
        })
    }

    pub fn blocked_sleep_duration(&self, native: u64) -> Option<Duration> {
        let thread = self.get_thread(native)?;
        if thread.native.phase != ThreadPhase::Active(RunState::Blocked)
            || thread.children.event_object.is_nil()
        {
            return None;
        }
        sleep_duration_from_blocker(thread.children.event_object)
    }

    pub fn wake_thread(&mut self, native: u64) {
        if self
            .get_thread(native)
            .is_some_and(|thread| thread.native.phase == ThreadPhase::Active(RunState::Blocked))
        {
            self.update_thread(native, |children| children.event_object = Value::NIL);
        }
    }

    pub fn take_blocked_remaining_forms(&mut self, native: u64) -> Option<Value> {
        if !self.thread_ready_to_resume(native) {
            return None;
        }
        let remaining = self.update_thread(native, |children| {
            std::mem::replace(&mut children.blocked_remaining_forms, Value::NIL)
        })?;
        self.update_thread_native(native, |state| {
            state.phase = ThreadPhase::Active(RunState::Running)
        });
        Some(remaining)
    }

    pub fn blocked_remaining_forms_if_ready(&self, native: u64) -> Option<Value> {
        let thread = self.get_thread(native)?;
        (thread.native.phase == ThreadPhase::Active(RunState::Blocked)
            && thread.children.event_object.is_nil())
        .then_some(thread.children.blocked_remaining_forms)
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
            .map(|t| t.children.buffer_disposition)
    }
    pub(crate) fn thread_buffer_disposition_for_value(&self, value: Value) -> Option<Value> {
        self.thread_payload(value)
            .map(|t| t.children.buffer_disposition)
    }
    pub fn set_thread_buffer_disposition(&mut self, native: u64, value: Value) -> bool {
        self.update_thread(native, |t| t.buffer_disposition = value)
            .is_some()
    }
    pub(crate) fn set_thread_buffer_disposition_for_value(
        &mut self,
        thread: Value,
        value: Value,
    ) -> bool {
        self.update_thread_value(thread, |t| t.buffer_disposition = value)
            .is_some()
    }
    pub fn thread_current_buffer(&self, native: u64) -> Option<crate::buffer::BufferId> {
        self.get_thread(native)
            .and_then(|t| t.native.current_buffer)
    }
    #[cfg(test)]
    pub(super) fn thread_current_buffer_for_value(
        &self,
        thread: Value,
    ) -> Option<crate::buffer::BufferId> {
        self.thread_payload(thread)
            .and_then(|t| t.native.current_buffer)
    }
    pub fn set_thread_current_buffer(
        &mut self,
        native: u64,
        buffer: Option<crate::buffer::BufferId>,
    ) -> bool {
        let Some(thread) = self.get_thread(native) else {
            return false;
        };
        if thread.native.current_buffer == buffer {
            return true;
        }
        self.update_thread_native(native, |state| state.current_buffer = buffer)
            .is_some()
    }
    pub fn thread_blocker(&self, native: u64) -> Option<Value> {
        self.get_thread(native).map(|t| t.children.event_object)
    }
    pub(crate) fn thread_blocker_for_value(&self, value: Value) -> Option<Value> {
        self.thread_payload(value).map(|t| t.children.event_object)
    }
    pub fn set_thread_blocker(&mut self, native: u64, blocker: Value) -> bool {
        self.update_thread(native, |t| t.event_object = blocker)
            .is_some()
    }
    pub fn clear_thread_blocker(&mut self, native: u64) -> bool {
        self.set_thread_blocker(native, Value::NIL)
    }

    pub fn mark_thread_exited(&mut self, native: u64) {
        self.update_thread_native(native, |state| {
            let outcome = match state.phase {
                ThreadPhase::New | ThreadPhase::Active(_) => ThreadOutcome::Returned,
                ThreadPhase::Completing(outcome) | ThreadPhase::Terminal(outcome) => outcome,
            };
            state.phase = ThreadPhase::Terminal(outcome);
        });
    }

    pub fn set_thread_error(&mut self, native: u64, symbol: Value, data: Value) {
        if let Some(thread) = self.live_value(native) {
            self.set_thread_error_for_value(thread, symbol, data);
        }
    }

    pub(crate) fn set_thread_error_for_value(&mut self, value: Value, symbol: Value, data: Value) {
        let error = Value::cons(symbol, data);
        self.update_thread_value(value, |thread| {
            thread.error_symbol = symbol;
            thread.error_data = data;
            thread.last_error = Some(error);
        });
    }

    pub fn take_thread_error(&mut self, native: u64) -> Option<(Value, Value)> {
        self.update_thread(native, |thread| {
            if thread.error_symbol.is_nil() {
                return None;
            }
            Some((
                std::mem::replace(&mut thread.error_symbol, Value::NIL),
                std::mem::replace(&mut thread.error_data, Value::NIL),
            ))
        })?
    }

    pub fn restore_thread_error(&mut self, native: u64, symbol: Value, data: Value) {
        self.update_thread(native, |thread| {
            if thread.error_symbol.is_nil() {
                thread.error_symbol = symbol;
                thread.error_data = data;
            }
        });
    }

    fn all_native_registrations(&self) -> Vec<u64> {
        self.threads.keys().map(|id| id.native_key()).collect()
    }

    pub fn buffer_held_by_other_thread(&self, buffer: crate::buffer::BufferId) -> bool {
        self.threads.iter().any(|(id, value)| {
            self.thread_payload(*value).is_some_and(|thread| {
                id.native_key() != self.current_thread.native_key()
                    && thread_is_running(thread)
                    && thread.native.current_buffer == Some(buffer)
                    && thread.children.buffer_disposition.is_t()
            })
        })
    }

    pub fn buffer_current_in_other_thread(&self, buffer: crate::buffer::BufferId) -> bool {
        self.threads.iter().any(|(id, value)| {
            self.thread_payload(*value).is_some_and(|thread| {
                id.native_key() != self.current_thread.native_key()
                    && thread_is_running(thread)
                    && thread.native.current_buffer == Some(buffer)
            })
        })
    }

    pub fn move_threads_off_killed_buffer(
        &mut self,
        buffer: crate::buffer::BufferId,
        other: crate::buffer::BufferId,
    ) {
        for native in self.all_native_registrations() {
            let Some(thread) = self.get_thread(native) else {
                continue;
            };
            if native == self.current_thread.native_key()
                || !thread_is_running(thread)
                || thread.native.current_buffer != Some(buffer)
            {
                continue;
            }
            let pending = if thread
                .children
                .buffer_disposition
                .is_bare_symbol_named("silently")
            {
                None
            } else {
                let symbol = Value::symbol("thread-buffer-killed");
                Some((symbol, Value::cons(symbol, Value::NIL)))
            };
            if let Some((symbol, error)) = pending {
                self.update_thread(native, |children| {
                    children.error_symbol = symbol;
                    children.error_data = Value::NIL;
                    children.last_error = Some(error);
                });
            }
            self.set_thread_current_buffer(native, Some(other));
        }
    }

    pub(crate) fn create_mutex(&mut self, name: Option<LispString>) -> Result<MutexHandle, Flow> {
        let id = self.ids.issue_mutex().map_err(identity_exhausted)?;
        let value = with_tagged_heap(|heap| {
            heap.alloc_mutex(MutexPayload {
                id,
                name,
                ownership: MutexOwnership::Unlocked,
            })
        });
        Ok(MutexHandle { value, id })
    }

    fn mutex_payload(&self, mutex: MutexHandle) -> Option<&MutexPayload> {
        let ptr = mutex.value.as_veclike_ptr()?.cast::<MutexObj>();
        // SAFETY: private handle constructors validate the rooted object's
        // exact tag and issued same-World identity. A borrowed view cannot cross
        // a Context safe point; no ownership edge to the native owner is added.
        Some(unsafe { &(*ptr).payload })
    }

    fn update_mutex<R>(
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

    pub(crate) fn mutex_id_from_handle(&self, value: &Value) -> Option<MutexHandle> {
        if value.veclike_type()? != VecLikeType::Mutex {
            return None;
        }
        let ptr = value.as_veclike_ptr()?.cast::<MutexObj>();
        // SAFETY: observed live Lisp Mutex value fixes the concrete layout.
        let id = unsafe { (*ptr).payload.id };
        (self.ids.mutex_from_native_key(id.native_key())? == id)
            .then_some(MutexHandle { value: *value, id })
    }

    pub(crate) fn is_mutex(&self, mutex: MutexHandle) -> bool {
        self.mutex_id_from_handle(&mutex.value).is_some()
    }
    pub(crate) fn mutex_handle(&self, mutex: MutexHandle) -> Value {
        mutex.value
    }
    pub(crate) fn mutex_name(&self, mutex: MutexHandle) -> Option<&LispString> {
        self.mutex_payload(mutex)?.name.as_ref()
    }
    pub(crate) fn mutex_free(&self, mutex: MutexHandle) -> bool {
        self.mutex_payload(mutex)
            .is_some_and(|payload| payload.ownership == MutexOwnership::Unlocked)
    }
    pub(crate) fn mutex_owned_by_current_thread(&self, mutex: MutexHandle) -> bool {
        self.mutex_payload(mutex)
            .is_some_and(|payload| match payload.ownership {
                MutexOwnership::Unlocked => false,
                MutexOwnership::Locked { owner, depth: _ } => {
                    owner.native_key() == self.current_thread.native_key()
                }
            })
    }

    pub(crate) fn mutex_lock(&mut self, mutex: MutexHandle) -> MutexAcquisition {
        let Some(current) = self
            .ids
            .thread_from_native_key(self.current_thread.native_key())
        else {
            return MutexAcquisition::Blocked;
        };
        self.update_mutex(mutex, |state| match state.ownership {
            MutexOwnership::Unlocked => {
                state.ownership = MutexOwnership::Locked {
                    owner: current,
                    depth: RecursionDepth::ONE,
                };
                MutexAcquisition::Acquired
            }
            MutexOwnership::Locked { owner, depth } if owner == current => {
                let Some(depth) = depth.checked_increment() else {
                    return MutexAcquisition::DepthExhausted;
                };
                state.ownership = MutexOwnership::Locked { owner, depth };
                MutexAcquisition::Acquired
            }
            MutexOwnership::Locked { .. } => MutexAcquisition::Blocked,
        })
        .unwrap_or(MutexAcquisition::Blocked)
    }

    pub(crate) fn mutex_unlock(&mut self, mutex: MutexHandle) -> MutexRelease {
        let current = self.current_thread.native_key();
        self.update_mutex(mutex, |state| match state.ownership {
            MutexOwnership::Unlocked => MutexRelease::NotOwner,
            MutexOwnership::Locked { owner, depth } if owner.native_key() == current => {
                match RecursionDepth::new(depth.get() - 1) {
                    Some(depth) => {
                        state.ownership = MutexOwnership::Locked { owner, depth };
                        MutexRelease::StillLocked
                    }
                    None => {
                        state.ownership = MutexOwnership::Unlocked;
                        MutexRelease::Released
                    }
                }
            }
            MutexOwnership::Locked { .. } => MutexRelease::NotOwner,
        })
        .unwrap_or(MutexRelease::NotOwner)
    }

    pub(crate) fn mutex_unlock_for_wait(&mut self, mutex: MutexHandle) -> Option<RecursionDepth> {
        let current = self.current_thread.native_key();
        self.update_mutex(mutex, |state| match state.ownership {
            MutexOwnership::Unlocked => None,
            MutexOwnership::Locked { owner, depth } if owner.native_key() == current => {
                state.ownership = MutexOwnership::Unlocked;
                Some(depth)
            }
            MutexOwnership::Locked { .. } => None,
        })?
    }

    pub(crate) fn mutex_relock_after_wait(
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
        self.update_mutex(mutex, |state| match state.ownership {
            MutexOwnership::Unlocked => {
                state.ownership = MutexOwnership::Locked { owner, depth };
                MutexAcquisition::Acquired
            }
            MutexOwnership::Locked { .. } => MutexAcquisition::Blocked,
        })
        .unwrap_or(MutexAcquisition::Blocked)
    }

    pub(crate) fn create_condition_variable(
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

    fn condition_payload(&self, condition: CondVarHandle) -> Option<&CondVarPayload> {
        let ptr = condition.value.as_veclike_ptr()?.cast::<CondVarObj>();
        // SAFETY: private handle construction fixes the rooted live layout;
        // the serial borrow excludes payload mutation/collection during view.
        Some(unsafe { &(*ptr).payload })
    }

    pub(crate) fn condition_variable_id_from_handle(&self, value: &Value) -> Option<CondVarHandle> {
        if value.veclike_type()? != VecLikeType::CondVar {
            return None;
        }
        let ptr = value.as_veclike_ptr()?.cast::<CondVarObj>();
        // SAFETY: observed live Lisp CondVar value fixes the concrete layout.
        let payload = unsafe { &(*ptr).payload };
        let id = payload.id;
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

    pub(crate) fn is_condition_variable(&self, condition: CondVarHandle) -> bool {
        self.condition_variable_id_from_handle(&condition.value)
            .is_some()
    }
    pub(crate) fn condition_variable_handle(&self, condition: CondVarHandle) -> Value {
        condition.value
    }
    pub(crate) fn condition_variable_name(&self, condition: CondVarHandle) -> Option<&LispString> {
        self.condition_payload(condition)?.name.as_ref()
    }
    pub(crate) fn condition_variable_mutex(&self, condition: CondVarHandle) -> MutexHandle {
        condition.mutex
    }
}

fn identity_exhausted(error: crate::heap_types::threading::NativeIdExhausted) -> Flow {
    signal("error", vec![Value::string(error.to_string())])
}

fn invalid_mutex() -> Flow {
    signal("error", vec![Value::string("Invalid mutex association")])
}

fn thread_is_running(thread: &ThreadPayload) -> bool {
    match thread.native.phase {
        ThreadPhase::New | ThreadPhase::Active(_) => true,
        ThreadPhase::Completing(_) | ThreadPhase::Terminal(_) => false,
    }
}

fn thread_is_live(thread: &ThreadPayload) -> bool {
    match thread.native.phase {
        ThreadPhase::New | ThreadPhase::Active(_) => true,
        ThreadPhase::Completing(ThreadOutcome::Returned) => {
            thread.native.join_observation == JoinObservation::Unobserved
        }
        ThreadPhase::Completing(ThreadOutcome::InternalError | ThreadOutcome::Signaled)
        | ThreadPhase::Terminal(_) => false,
    }
}

impl ThreadManager {
    /// Refusal preserves both the target and the caller's temporary leases.
    pub(crate) fn queue_thread_event(
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
                heap.with_thread_payload_mut(value, |children| children.stored_events.push(stored))
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
    pub(crate) fn take_thread_events(
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
        if payload.children.stored_events.is_empty() {
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
                .children
                .stored_events
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
        self.update_thread_value(value, |children| children.stored_events.clear())
            .ok_or(MailboxError::MissingTarget)?;
        if target == self.ids.main_thread() {
            self.mailbox_reservation.main = layout::MainMailboxState::Empty;
        }
        Ok(receiving)
    }
}
