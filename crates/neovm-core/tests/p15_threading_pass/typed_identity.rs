#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;

// The included production source names the crate-root vocabulary.
use actual_payload::{buffer, tagged, threading};
use threading::{
    CondVarId, JoinObservation, MutexId, RunState, StoredThreadEvent, ThreadId, ThreadIds,
    ThreadName, ThreadOutcome, ThreadPayload, ThreadPhase,
};

fn accept_thread(_: ThreadId) {}
fn accept_mutex(_: MutexId) {}
fn accept_condition(_: CondVarId) {}

fn transferable_native_ids() {
    fn require_send<T: Send>() {}
    require_send::<ThreadId>();
    require_send::<MutexId>();
    require_send::<CondVarId>();
}

fn typed_id_and_schema_client(mut ids: ThreadIds) {
    accept_thread(ids.issue_thread().unwrap());
    accept_mutex(ids.issue_mutex().unwrap());
    accept_condition(ids.issue_condvar().unwrap());
    let mut payload = ThreadPayload {
        id: ids.main_thread(),
        name: ThreadName::UNNAMED,
        function: tagged::value::TaggedValue::NIL,
        phase: ThreadPhase::New,
        result: tagged::value::TaggedValue::NIL,
        last_error: None,
        join_observation: JoinObservation::Unobserved,
        buffer_disposition: tagged::value::TaggedValue::NIL,
        current_buffer: None,
        event_object: tagged::value::TaggedValue::NIL,
        error_symbol: tagged::value::TaggedValue::NIL,
        error_data: tagged::value::TaggedValue::NIL,
        blocked_remaining_forms: tagged::value::TaggedValue::NIL,
        stored_events: Vec::<StoredThreadEvent>::new(),
    };
    // There is one exhaustive production child schema; clients consume it.
    payload.visit_children(&mut |_| {});
    assert!(payload.logical_backing_bytes() <= payload.backing_bytes());
    payload.phase = match payload.phase {
        ThreadPhase::New => ThreadPhase::Active(RunState::Running),
        ThreadPhase::Active(RunState::Running | RunState::Blocked) => {
            ThreadPhase::Completing(ThreadOutcome::Returned)
        }
        ThreadPhase::Completing(outcome) => ThreadPhase::Terminal(outcome),
        ThreadPhase::Terminal(outcome) => ThreadPhase::Terminal(outcome),
    };
}

fn main() {
    // Type-check the client without fabricating heap identities or payload
    // authority at runtime. Creating an issuer requires a real HeapIdentity.
    let _: fn(ThreadIds) = typed_id_and_schema_client;
}
