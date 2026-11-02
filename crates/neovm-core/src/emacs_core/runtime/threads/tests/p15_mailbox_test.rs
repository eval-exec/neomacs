//! Target-owned THREAD_EVENT roots and atomic same-World admission.
//!
//! GNU thread.c:1011-1021 retains the event in kbd_buffer until consumption;
//! thread.c:853-859 unlinks a completed worker independently of that event.
//! Cross-World refusal is Neo's transport invariant, not a GNU condition.

use super::*;
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::threads::mailbox::MailboxError;
use crate::tagged::gc::TaggedHeap;
use crate::tagged::transport::{SharedRoot, SharedRootError, collect_shared_root_gc_roots};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EventWords([usize; 3]);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EventCapacity(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AllocationDebt(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RememberedOwners(usize);

#[derive(Debug, PartialEq, Eq)]
struct MailboxSnapshot {
    events: Vec<EventWords>,
    capacity: EventCapacity,
    debt: AllocationDebt,
    remembered: RememberedOwners,
    external_roots: Vec<usize>,
}

fn external_roots(heap: &TaggedHeap) -> Vec<usize> {
    let mut roots = Vec::new();
    collect_shared_root_gc_roots(heap, &mut roots);
    let mut words: Vec<_> = roots.into_iter().map(Value::bits).collect();
    words.sort_unstable();
    words
}

fn snapshot(context: &Context, target: ThreadId) -> MailboxSnapshot {
    let children = &context
        .threads
        .get_thread(target.native_key())
        .unwrap()
        .children;
    MailboxSnapshot {
        events: children
            .stored_events
            .iter()
            .map(|event| EventWords(event.values().map(Value::bits)))
            .collect(),
        capacity: EventCapacity(children.stored_events.capacity()),
        debt: AllocationDebt(context.tagged_heap.bytes_since_gc_exact()),
        remembered: RememberedOwners(context.tagged_heap.remembered_log_len_for_test()),
        external_roots: external_roots(&context.tagged_heap),
    }
}

/// All words come from this installed World; no collection or yield occurs
/// between their allocation and their transport registration.
fn share(context: &Context, values: [Value; 3]) -> InFlightThreadEvent {
    // SAFETY: callers supply live values allocated by this Context, and keep
    // exclusive World authority until all three leases have been installed.
    unsafe { InFlightThreadEvent::share(context, values[0], values[1], values[2]) }.unwrap()
}

fn world() -> Context {
    let mut context = Context::new_with_threading(ThreadingMode::Sim);
    context.gc_stress = false;
    context.gc_collect_exact();
    context.tagged_heap.set_gc_threshold(usize::MAX);
    context
}

fn vectors() -> [Value; 3] {
    std::array::from_fn(|index| Value::vector(vec![Value::fixnum(710 + index as i64)]))
}

fn assert_vectors_live(context: &Context, values: [Value; 3]) {
    for value in values {
        assert!(
            context.tagged_heap.owns_heap_value_for_test(value),
            "mailbox-only child reclaimed"
        );
    }
    // Every allocation-membership assertion precedes every payload decode.
    for (index, value) in values.into_iter().enumerate() {
        assert_eq!(
            value.as_vector_data().unwrap().as_slice(),
            &[Value::fixnum(710 + index as i64)]
        );
    }
}

#[derive(Clone, Copy, Debug)]
enum ForeignField {
    Sender,
    ErrorSymbol,
    Data,
}

fn refuses_foreign_field(field: ForeignField) {
    let mut context = world();
    let target = context.threads.ids.main_thread();
    // Populate a real mailbox first, so refusal must preserve its existing
    // contents and capacity, rather than merely leave an empty Vec empty.
    let retained = share(&context, vectors());
    context
        .threads
        .queue_thread_event(target, &retained)
        .unwrap();
    let mut event = share(&context, vectors());
    let mut foreign = TaggedHeap::new();
    let foreign_value = foreign.alloc_vector(vec![Value::fixnum(991)]);
    // SAFETY: this test is the sole mutator of its separately owned heap;
    // its newly allocated vector stays live until registration completes.
    let foreign_root = unsafe { SharedRoot::new(&foreign, foreign_value) };
    match field {
        ForeignField::Sender => event.sender = foreign_root,
        ForeignField::ErrorSymbol => event.error_symbol = foreign_root,
        ForeignField::Data => event.data = foreign_root,
    }
    let foreign_before = external_roots(&foreign);
    assert!(context.tagged_heap.take_satb_shared_for_test().is_empty());
    context.tagged_heap.set_concurrent_active_for_test(true);
    let before = snapshot(&context, target);
    let result = context.threads.queue_thread_event(target, &event);
    let after = snapshot(&context, target);
    let preimages = context.tagged_heap.take_satb_shared_for_test();
    context.tagged_heap.set_concurrent_active_for_test(false);

    assert_eq!(
        result,
        Err(MailboxError::Transport(SharedRootError::ForeignHeap {
            owner: foreign.heap_identity(),
            mutator: context.tagged_heap.heap_identity(),
        })),
        "{field:?}"
    );
    assert_eq!(
        after, before,
        "{field:?} refusal changed target, debt, remembered owners or transport leases"
    );
    assert!(
        preimages.is_empty(),
        "{field:?} refusal recorded a heap write"
    );
    assert_eq!(external_roots(&foreign), foreign_before);
    assert!(foreign.owns_heap_value_for_test(foreign_value));
    // Retire the transport before its owning standalone heap is destroyed.
    drop(event);
    drop(retained);
}

#[test]
fn p15_mailbox_foreign_sender_is_atomic() {
    refuses_foreign_field(ForeignField::Sender);
}

#[test]
fn p15_mailbox_foreign_error_symbol_is_atomic() {
    refuses_foreign_field(ForeignField::ErrorSymbol);
}

#[test]
fn p15_mailbox_foreign_data_is_atomic() {
    refuses_foreign_field(ForeignField::Data);
}

fn complete(context: &mut Context, target: ThreadId) {
    context
        .threads
        .finish_thread(target.native_key(), Value::NIL);
    let completion = context
        .threads
        .prepare_thread_completion(target, CompletionExit::Simulated)
        .unwrap();
    let retired = context.threads.retire_thread_completion(completion);
    assert_eq!(retired.into_thread_id(), target);
}

/// GNU's late unlink does not promote ordinary thread fields into native roots
/// (thread.c:853-859). A self-referential mailbox is an ordinary dead cycle.
#[test]
fn p15_mailbox_stored_self_cycle_dies_after_completion() {
    let mut context = world();
    let baseline_roots = external_roots(&context.tagged_heap);
    let target = context
        .threads
        .try_create_thread(Value::NIL, ThreadName::UNNAMED)
        .unwrap();
    let thread = context.threads.thread_handle(target.native_key()).unwrap();
    let event = share(&context, [thread, Value::NIL, thread]);
    context.threads.queue_thread_event(target, &event).unwrap();
    drop(event);
    assert_eq!(external_roots(&context.tagged_heap), baseline_roots);
    context.gc_collect_exact();
    assert!(context.tagged_heap.owns_heap_value_for_test(thread));
    complete(&mut context, target);
    context.gc_collect_exact();
    assert!(
        !context.tagged_heap.owns_heap_value_for_test(thread),
        "stored cycle acquired a native root"
    );
    assert_eq!(external_roots(&context.tagged_heap), baseline_roots);
    assert_eq!(context.threads.threads.len(), 1);
}

/// GNU queues main-thread events in kbd_buffer (thread.c:1011-1021), so the
/// sender survives worker exit until the command reader consumes its event.
#[test]
fn p15_mailbox_main_event_alone_retains_completed_sender_until_consumption() {
    let mut context = world();
    let baseline_roots = external_roots(&context.tagged_heap);
    let target = context.threads.ids.main_thread();
    let worker = context
        .threads
        .try_create_thread(Value::NIL, ThreadName::UNNAMED)
        .unwrap();
    let sender = context.threads.thread_handle(worker.native_key()).unwrap();
    let data = Value::vector(vec![Value::fixnum(912)]);
    let event = share(&context, [sender, Value::NIL, data]);
    context.threads.queue_thread_event(target, &event).unwrap();
    drop(event);
    complete(&mut context, worker);
    assert_eq!(context.threads.threads.len(), 1);
    assert_eq!(external_roots(&context.tagged_heap), baseline_roots);
    context.gc_collect_exact();
    assert!(context.tagged_heap.owns_heap_value_for_test(sender));
    assert!(context.tagged_heap.owns_heap_value_for_test(data));
    assert_eq!(
        data.as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(912)]
    );

    context.stage_pending_thread_events().unwrap();
    assert!(
        context
            .threads
            .get_thread(0)
            .unwrap()
            .children
            .stored_events
            .is_empty()
    );
    context.gc_collect_exact();
    assert!(context.tagged_heap.owns_heap_value_for_test(sender));
    assert!(context.tagged_heap.owns_heap_value_for_test(data));
    let input = context
        .command_loop
        .keyboard
        .pending_input_events
        .pop_visible_front()
        .unwrap();
    let crate::keyboard::InputEvent::ThreadEvent {
        sender: queued_sender,
        error_symbol,
        data: queued_data,
    } = &input
    else {
        panic!("expected the queued THREAD_EVENT, got {input:?}");
    };
    assert_eq!(
        queued_sender
            .materialize(&context.tagged_heap)
            .unwrap()
            .value(),
        sender
    );
    assert!(
        error_symbol
            .materialize(&context.tagged_heap)
            .unwrap()
            .value()
            .is_nil()
    );
    assert_eq!(
        queued_data
            .materialize(&context.tagged_heap)
            .unwrap()
            .value(),
        data
    );
    drop(input);
    assert_eq!(external_roots(&context.tagged_heap), baseline_roots);
    context.gc_collect_exact();
    assert!(!context.tagged_heap.owns_heap_value_for_test(sender));
    assert!(!context.tagged_heap.owns_heap_value_for_test(data));
}

#[test]
fn p15_mailbox_completed_target_refuses_admission_without_consuming_leases() {
    let mut context = world();
    let target = context
        .threads
        .try_create_thread(Value::NIL, ThreadName::UNNAMED)
        .unwrap();
    let event = share(&context, vectors());
    complete(&mut context, target);
    let roots_before = external_roots(&context.tagged_heap);
    let debt_before = context.tagged_heap.bytes_since_gc_exact();
    assert_eq!(
        context.threads.queue_thread_event(target, &event),
        Err(MailboxError::MissingTarget)
    );
    assert_eq!(external_roots(&context.tagged_heap), roots_before);
    assert_eq!(context.tagged_heap.bytes_since_gc_exact(), debt_before);
    context.gc_collect_exact();
    for root in [&event.sender, &event.error_symbol, &event.data] {
        let value = root.materialize(&context.tagged_heap).unwrap().value();
        assert!(context.tagged_heap.owns_heap_value_for_test(value));
    }
}

fn minor_or_full(context: &mut Context) {
    if !context.tagged_heap.generational_enabled() {
        context.gc_collect_exact();
        return;
    }
    assert!(context.tagged_heap.should_run_minor(false, false));
    let before = context.gc_count;
    context.gc_pending = true;
    context.gc_safe_point_exact();
    assert!(
        context.tagged_heap.sweep_in_progress(),
        "mailbox fixture must select a minor"
    );
    assert!(!context.tagged_heap.concurrent_mark_running());
    for _ in 0..10_000 {
        if !context.tagged_heap.sweep_in_progress() {
            break;
        }
        context.gc_safe_point_exact();
    }
    assert!(!context.tagged_heap.sweep_in_progress());
    assert!(!context.tagged_heap.mark_in_progress());
    assert_eq!(context.gc_count, before + 1);
}

/// The main object is old and each event field is a distinct young sole edge.
/// Omitting the canonical owner write barrier loses them in the first minor.
#[test]
fn p15_mailbox_old_target_remembers_all_young_event_children() {
    let mut context = world();
    let target = context.threads.ids.main_thread();
    let values = vectors();
    let event = share(&context, values);
    context.threads.queue_thread_event(target, &event).unwrap();
    drop(event);
    // GEN1: minor -> minor -> full -> minor. GEN0: full at every step.
    minor_or_full(&mut context);
    assert_vectors_live(&context, values);
    minor_or_full(&mut context);
    assert_vectors_live(&context, values);
    context.gc_collect_exact();
    assert_vectors_live(&context, values);
    minor_or_full(&mut context);
    assert_vectors_live(&context, values);
    let receipt = context.threads.take_thread_events(target).unwrap();
    assert_eq!(receipt.len(), 1);
    drop(receipt);
    context.gc_collect_exact();
    for value in values {
        assert!(!context.tagged_heap.owns_heap_value_for_test(value));
    }
}

/// Clearing the stored vector must log its old children before mutation,
/// independently of the delivery leases that also keep them temporarily live.
#[test]
fn p15_mailbox_delivery_records_every_stored_satb_preimage() {
    let mut context = world();
    let target = context.threads.ids.main_thread();
    let values = vectors();
    let event = share(&context, values);
    context.threads.queue_thread_event(target, &event).unwrap();
    drop(event);
    context.gc_collect_exact();
    assert_vectors_live(&context, values);
    assert!(context.tagged_heap.take_satb_shared_for_test().is_empty());
    context.tagged_heap.set_concurrent_active_for_test(true);
    let receipt = context.threads.take_thread_events(target).unwrap();
    let preimages = context.tagged_heap.take_satb_shared_for_test();
    context.tagged_heap.set_concurrent_active_for_test(false);
    for value in values {
        assert!(
            preimages.contains(&value),
            "mailbox clear omitted a stored SATB preimage"
        );
    }
    assert_eq!(receipt.len(), 1);
    drop(receipt);
    context.gc_collect_exact();
    for value in values {
        assert!(!context.tagged_heap.owns_heap_value_for_test(value));
    }
}
