//! Live registrations and access to object-owned logical state.

use super::*;
use crate::tagged::gc::with_tagged_heap;
use crate::tagged::header::{CondVarObj, MutexObj, ThreadObj, VecLikeType};

impl ThreadManager {
    pub fn new() -> Self {
        let ids = ThreadIds::new(with_tagged_heap(|heap| heap.heap_identity()));
        let main = ids.main_thread();
        let value = with_tagged_heap(|heap| {
            heap.alloc_thread(ThreadPayload {
                id: main,
                name: crate::heap_types::threading::ThreadName::UNNAMED,
                function: Value::NIL,
                phase: ThreadPhase::Active(RunState::Running),
                result: Value::NIL,
                last_error: None,
                join_observation: JoinObservation::Unobserved,
                buffer_disposition: Value::NIL,
                current_buffer: None,
                event_object: Value::NIL,
                error_symbol: Value::NIL,
                error_data: Value::NIL,
                blocked_remaining_forms: Value::NIL,
                stored_events: Vec::new(),
            })
        });
        let mut threads = HashMap::new();
        threads.insert(main, value);
        Self {
            last_error: None,
            threads,
            transport_mailboxes: HashMap::new(),
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
            heap.alloc_thread(ThreadPayload {
                id,
                name: name.0,
                function,
                phase: ThreadPhase::New,
                result: Value::NIL,
                last_error: None,
                join_observation: JoinObservation::Unobserved,
                buffer_disposition: Value::NIL,
                current_buffer: None,
                event_object: Value::NIL,
                error_symbol: Value::NIL,
                error_data: Value::NIL,
                blocked_remaining_forms: Value::NIL,
                stored_events: Vec::new(),
            })
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
        let issued = self.ids.thread_from_native_key(payload.id.native_key())?;
        (issued == payload.id).then_some(payload)
    }

    pub(crate) fn get_thread(&self, native: u64) -> Option<&ThreadPayload> {
        self.thread_payload(self.live_value(native)?)
    }

    fn update_thread<R>(
        &mut self,
        native: u64,
        update: impl FnOnce(&mut ThreadPayload) -> R,
    ) -> Option<R> {
        self.update_thread_value(self.live_value(native)?, update)
    }

    fn update_thread_value<R>(
        &mut self,
        value: Value,
        update: impl FnOnce(&mut ThreadPayload) -> R,
    ) -> Option<R> {
        self.thread_payload(value)?;
        with_tagged_heap(|heap| {
            // SAFETY: value is a validated rooted Lisp argument or live-map
            // edge; the manager is exclusively borrowed under serial World
            // authority. Updates below never allocate Lisp, collect or yield.
            unsafe { heap.with_thread_payload_mut(value, update) }
        })
    }

    pub fn start_thread(&mut self, native: u64) {
        self.update_thread(native, |thread| {
            thread.phase = ThreadPhase::Active(RunState::Running)
        });
    }

    pub fn discard_unstarted_thread(&mut self, native: u64) {
        if let Some(id) = self.ids.thread_from_native_key(native) {
            self.transport_mailboxes.remove(&id);
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
        self.update_thread(native, |thread| {
            thread.phase = ThreadPhase::Completing(ThreadOutcome::Returned);
            thread.result = result;
        });
    }

    pub fn signal_thread(&mut self, native: u64, error: Value) {
        let signal = split_signal_binding_value(error);
        self.update_thread(native, |thread| {
            thread.last_error = Some(error);
            let (symbol, data) = signal.unwrap_or((Value::NIL, Value::NIL));
            thread.error_symbol = symbol;
            thread.error_data = data;
            match thread.phase {
                ThreadPhase::Active(RunState::Blocked) => thread.event_object = Value::NIL,
                ThreadPhase::New
                | ThreadPhase::Active(RunState::Running)
                | ThreadPhase::Completing(_)
                | ThreadPhase::Terminal(_) => {
                    thread.phase = ThreadPhase::Completing(ThreadOutcome::Signaled);
                }
            }
        });
    }

    pub fn record_thread_internal_error(&mut self, native: u64, error: Value) {
        self.update_thread(native, |thread| {
            thread.phase = ThreadPhase::Completing(ThreadOutcome::InternalError);
            thread.last_error = Some(error);
            thread.error_symbol = Value::NIL;
            thread.error_data = Value::NIL;
        });
    }

    pub fn pending_thread_signal(&self, native: u64) -> Option<Flow> {
        self.pending_thread_signal_for_value(self.live_value(native)?)
    }

    pub(crate) fn pending_thread_signal_for_value(&self, value: Value) -> Option<Flow> {
        let thread = self.thread_payload(value)?;
        if thread.error_symbol.is_nil() {
            return None;
        }
        thread.last_error.and_then(signal_from_binding_value)
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
            .map(|thread| ThreadName(thread.name))
    }

    pub(crate) fn thread_name_for_value(&self, value: Value) -> Option<ThreadName> {
        self.thread_payload(value)
            .map(|thread| ThreadName(thread.name))
    }

    pub fn queue_thread_event(&mut self, native: u64, event: PendingThreadEvent) {
        if let Some(id) = self.ids.thread_from_native_key(native)
            && self.threads.contains_key(&id)
        {
            self.transport_mailboxes.entry(id).or_default().push(event);
        }
    }

    pub fn take_thread_events(&mut self, native: u64) -> Vec<PendingThreadEvent> {
        self.ids
            .thread_from_native_key(native)
            .and_then(|id| self.transport_mailboxes.remove(&id))
            .unwrap_or_default()
    }

    pub fn is_thread(&self, native: u64) -> bool {
        self.live_value(native).is_some()
    }
    pub fn thread_handle(&self, native: u64) -> Option<Value> {
        self.live_value(native)
    }
    pub fn thread_id_from_handle(&self, value: &Value) -> Option<u64> {
        self.thread_payload(*value)
            .map(|thread| thread.id.native_key())
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
            .map_or(Value::NIL, |thread| thread.result)
    }

    pub(crate) fn thread_result_for_value(&self, value: Value) -> Value {
        self.thread_payload(value)
            .map_or(Value::NIL, |thread| thread.result)
    }

    pub fn join_thread(&mut self, native: u64) -> Option<Value> {
        self.join_thread_for_value(self.live_value(native)?)
    }

    pub(crate) fn join_thread_for_value(&mut self, value: Value) -> Option<Value> {
        let (id, terminal, error) = self.update_thread_value(value, |thread| {
            thread.join_observation = JoinObservation::Observed;
            let terminal = match thread.phase {
                ThreadPhase::New | ThreadPhase::Active(_) => false,
                ThreadPhase::Completing(_) | ThreadPhase::Terminal(_) => true,
            };
            (thread.id, terminal, thread.last_error)
        })?;
        if terminal && id.native_key() != 0 {
            // S1 retires completed joins. S2 moves this release to the final
            // native completion boundary, covering unjoined workers as well.
            self.transport_mailboxes.remove(&id);
            self.threads.remove(&id);
        }
        error
    }

    pub fn block_thread(&mut self, native: u64, blocker: Value, remaining: Value) {
        self.update_thread(native, |thread| {
            thread.phase = ThreadPhase::Active(RunState::Blocked);
            thread.event_object = blocker;
            thread.blocked_remaining_forms = remaining;
        });
    }

    pub fn wake_threads_blocked_on(&mut self, blocker: Value) {
        let ids: Vec<_> = self.all_native_registrations();
        for id in ids {
            self.update_thread(id, |thread| {
                if thread.phase == ThreadPhase::Active(RunState::Blocked)
                    && eq_value(&thread.event_object, &blocker)
                {
                    thread.event_object = Value::NIL;
                }
            });
        }
    }

    pub fn thread_ready_to_resume(&self, native: u64) -> bool {
        self.get_thread(native).is_some_and(|thread| {
            thread.phase == ThreadPhase::Active(RunState::Blocked) && thread.event_object.is_nil()
        })
    }

    pub fn blocked_sleep_duration(&self, native: u64) -> Option<Duration> {
        let thread = self.get_thread(native)?;
        if thread.phase != ThreadPhase::Active(RunState::Blocked) || thread.event_object.is_nil() {
            return None;
        }
        sleep_duration_from_blocker(thread.event_object)
    }

    pub fn wake_thread(&mut self, native: u64) {
        self.update_thread(native, |thread| {
            if thread.phase == ThreadPhase::Active(RunState::Blocked) {
                thread.event_object = Value::NIL;
            }
        });
    }

    pub fn take_blocked_remaining_forms(&mut self, native: u64) -> Option<Value> {
        self.update_thread(native, |thread| {
            if thread.phase != ThreadPhase::Active(RunState::Blocked)
                || !thread.event_object.is_nil()
            {
                return None;
            }
            let remaining = std::mem::replace(&mut thread.blocked_remaining_forms, Value::NIL);
            thread.phase = ThreadPhase::Active(RunState::Running);
            Some(remaining)
        })?
    }

    pub fn blocked_remaining_forms_if_ready(&self, native: u64) -> Option<Value> {
        let thread = self.get_thread(native)?;
        (thread.phase == ThreadPhase::Active(RunState::Blocked) && thread.event_object.is_nil())
            .then_some(thread.blocked_remaining_forms)
    }

    pub fn last_error(&mut self, cleanup: bool) -> Value {
        let value = self.last_error.unwrap_or(Value::NIL);
        if cleanup {
            self.last_error = None;
        }
        value
    }

    pub fn thread_buffer_disposition(&self, native: u64) -> Option<Value> {
        self.get_thread(native).map(|t| t.buffer_disposition)
    }
    pub(crate) fn thread_buffer_disposition_for_value(&self, value: Value) -> Option<Value> {
        self.thread_payload(value).map(|t| t.buffer_disposition)
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
        self.get_thread(native).and_then(|t| t.current_buffer)
    }
    #[cfg(test)]
    pub(super) fn thread_current_buffer_for_value(
        &self,
        thread: Value,
    ) -> Option<crate::buffer::BufferId> {
        self.thread_payload(thread).and_then(|t| t.current_buffer)
    }
    pub fn set_thread_current_buffer(
        &mut self,
        native: u64,
        buffer: Option<crate::buffer::BufferId>,
    ) -> bool {
        self.update_thread(native, |t| t.current_buffer = buffer)
            .is_some()
    }
    pub fn thread_blocker(&self, native: u64) -> Option<Value> {
        self.get_thread(native).map(|t| t.event_object)
    }
    pub(crate) fn thread_blocker_for_value(&self, value: Value) -> Option<Value> {
        self.thread_payload(value).map(|t| t.event_object)
    }
    pub fn set_thread_blocker(&mut self, native: u64, blocker: Value) -> bool {
        self.update_thread(native, |t| t.event_object = blocker)
            .is_some()
    }
    pub fn clear_thread_blocker(&mut self, native: u64) -> bool {
        self.set_thread_blocker(native, Value::NIL)
    }

    pub fn mark_thread_exited(&mut self, native: u64) {
        self.update_thread(native, |thread| {
            let outcome = match thread.phase {
                ThreadPhase::New | ThreadPhase::Active(_) => ThreadOutcome::Returned,
                ThreadPhase::Completing(outcome) | ThreadPhase::Terminal(outcome) => outcome,
            };
            thread.phase = ThreadPhase::Terminal(outcome);
        });
    }

    pub fn set_thread_error(&mut self, native: u64, symbol: Value, data: Value) {
        let error = Value::cons(symbol, data);
        self.update_thread(native, |thread| {
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
                    && thread.current_buffer == Some(buffer)
                    && thread.buffer_disposition.is_t()
            })
        })
    }

    pub fn buffer_current_in_other_thread(&self, buffer: crate::buffer::BufferId) -> bool {
        self.threads.iter().any(|(id, value)| {
            self.thread_payload(*value).is_some_and(|thread| {
                id.native_key() != self.current_thread.native_key()
                    && thread_is_running(thread)
                    && thread.current_buffer == Some(buffer)
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
                || thread.current_buffer != Some(buffer)
            {
                continue;
            }
            let pending = if thread.buffer_disposition.is_bare_symbol_named("silently") {
                None
            } else {
                let symbol = Value::symbol("thread-buffer-killed");
                Some((symbol, Value::cons(symbol, Value::NIL)))
            };
            self.update_thread(native, |thread| {
                if let Some((symbol, error)) = pending {
                    thread.error_symbol = symbol;
                    thread.error_data = Value::NIL;
                    thread.last_error = Some(error);
                }
                thread.current_buffer = Some(other);
            });
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
    match thread.phase {
        ThreadPhase::New | ThreadPhase::Active(_) => true,
        ThreadPhase::Completing(_) | ThreadPhase::Terminal(_) => false,
    }
}

fn thread_is_live(thread: &ThreadPayload) -> bool {
    match thread.phase {
        ThreadPhase::New | ThreadPhase::Active(_) => true,
        ThreadPhase::Completing(ThreadOutcome::Returned) => {
            thread.join_observation == JoinObservation::Unobserved
        }
        ThreadPhase::Completing(ThreadOutcome::InternalError | ThreadOutcome::Signaled)
        | ThreadPhase::Terminal(_) => false,
    }
}
