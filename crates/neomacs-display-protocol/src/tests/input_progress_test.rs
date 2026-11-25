use super::*;

fn stream(raw: u64) -> InputStream {
    InputStream::new(InputStreamId::new(NonZeroU64::new(raw).unwrap()))
}

fn disposition(receipt: InputReceipt, progress: &InputProgress) -> Disposition {
    Disposition::of(receipt, &progress.checkpoint())
}

#[test]
fn preview_observation_belongs_only_to_innermost_command() {
    let stream = stream(1);
    let receipt = stream.issue().unwrap().receipt();
    let mut progress = InputProgress::default();
    progress.consumed(receipt);
    assert!(progress.current_command_receipts().is_empty());
    let outer = progress.begin_command();
    assert_eq!(progress.current_command_receipts(), [receipt]);
    let inner = progress.begin_command();
    assert!(progress.current_command_receipts().is_empty());
    drop(inner);
    assert_eq!(progress.current_command_receipts().len(), 1);
    drop(outer);
    assert!(progress.current_command_receipts().is_empty());
    assert_eq!(disposition(receipt, &progress), Disposition::Reflected);
}

#[test]
fn old_frame_cannot_acknowledge_a_later_command_completion() {
    let stream = stream(1);
    let receipt = stream.issue().unwrap().receipt();
    let mut progress = InputProgress::default();
    progress.consumed(receipt);
    let command = progress.begin_command();
    let old_frame = progress.checkpoint();
    assert_eq!(Disposition::of(receipt, &old_frame), Disposition::Partial);
    drop(command);
    assert_eq!(Disposition::of(receipt, &old_frame), Disposition::Partial);
    assert_eq!(disposition(receipt, &progress), Disposition::Reflected);
}

/// Re-derived for the exact frontier (DESIGN §6.14). Before it, a completed
/// inner command was not acknowledged until the outer one finished. Now the
/// inner input is reflected at once, while the completed prefix that
/// presentations are keyed on still waits for the outer command.
#[test]
fn nested_completion_waits_for_the_outer_command() {
    let stream = stream(1);
    let outer = stream.issue().unwrap().receipt();
    let inner = stream.issue().unwrap().receipt();
    let mut progress = InputProgress::default();
    progress.consumed(outer);
    let outer_command = progress.begin_command();
    let inner_command = progress.begin_command();
    progress.consumed(inner);
    drop(inner_command);
    assert_eq!(disposition(inner, &progress), Disposition::Reflected);
    assert_eq!(disposition(outer, &progress), Disposition::Partial);
    assert_eq!(progress.completed_prefixes()[0].through, 0);
    drop(outer_command);
    assert_eq!(disposition(outer, &progress), Disposition::Reflected);
    assert_eq!(progress.completed_prefixes()[0].through, 2);
}

#[test]
fn completion_on_unwind_and_stream_identity_are_independent() {
    let receipt = stream(1).issue().unwrap().receipt();
    let foreign = stream(2).issue().unwrap().receipt();
    let mut progress = InputProgress::default();
    progress.consumed(receipt);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _command = progress.begin_command();
        panic!("command aborted");
    }));
    assert_eq!(disposition(receipt, &progress), Disposition::Reflected);
    assert_eq!(disposition(foreign, &progress), Disposition::Pending);
}

/// Re-derived: cancellation is the explicit `InputCancelled`, not the drop of
/// the last queued copy. Discarding states it; the compositor's observing
/// receipt sees it through the stream's ledger.
#[test]
fn discarding_delivery_cancels_even_while_the_compositor_keeps_a_receipt() {
    let stream = stream(1);
    let delivery = stream.issue().unwrap();
    let observer = delivery.receipt();
    let queued_copy = delivery.clone();
    drop(delivery);
    assert!(!stream.cancelled(observer));
    assert_eq!(queued_copy.cancel(), InputCancelled(observer));
    assert!(stream.cancelled(observer));
    drop(queued_copy);
    assert!(stream.cancelled(observer));
}

#[test]
fn completed_delivery_does_not_later_turn_into_cancellation() {
    let stream = stream(1);
    let delivery = stream.issue().unwrap();
    let observer = delivery.receipt();
    let mut progress = InputProgress::default();
    let command = progress.begin_command();
    progress.consumed(delivery.receipt());
    assert_eq!(disposition(observer, &progress), Disposition::Partial);
    drop(command);
    drop(delivery);
    assert_eq!(disposition(observer, &progress), Disposition::Reflected);
    assert!(!stream.cancelled(observer));
}

/// Capacity returns when the frontend observes a frame whose frontier passed
/// the inputs, the only completion signal a frontend has across a link.
#[test]
fn pending_receipts_are_bounded_and_capacity_returns_after_completion() {
    let stream = stream(1);
    let receipts: Vec<_> = (0..MAX_PENDING)
        .map(|_| stream.issue().unwrap().receipt())
        .collect();
    assert!(stream.issue().is_none());
    let mut progress = InputProgress::default();
    for receipt in receipts {
        let _command = progress.begin_command();
        progress.consumed(receipt);
    }
    assert!(
        stream.issue().is_none(),
        "no frame has reported completion yet"
    );
    stream.observe(&progress.checkpoint());
    assert!(stream.issue().is_some());
}

/// An input the evaluator consumed is no longer in flight, even while the
/// command it started still runs: the frontier's `in_progress` carries its
/// completion, so the ledger's capacity bounds only undelivered input.
#[test]
fn consumed_input_returns_capacity_while_its_command_runs() {
    let stream = stream(1);
    let receipts: Vec<_> = (0..MAX_PENDING)
        .map(|_| stream.issue().unwrap().receipt())
        .collect();
    assert!(stream.issue().is_none());
    let mut progress = InputProgress::default();
    let long_command = progress.begin_command();
    for receipt in &receipts {
        progress.consumed(*receipt);
    }
    let frame = progress.checkpoint();
    assert_eq!(Disposition::of(receipts[0], &frame), Disposition::Partial);
    stream.observe(&frame);
    assert!(
        stream.issue().is_some(),
        "consumed input no longer occupies the frontend's ledger"
    );
    drop(long_command);
}

/// A run of cancelled inputs at the head of the ledger is settled at once, as
/// the old shared ledger advanced past a cancellation: no later tracked input
/// has to be consumed before tracking resumes. The ledger still reports each
/// cancellation until a frame passes it.
#[test]
fn trailing_cancellations_return_capacity_and_stay_cancelled() {
    let stream = stream(1);
    let deliveries: Vec<_> = (0..MAX_PENDING).map(|_| stream.issue().unwrap()).collect();
    assert!(stream.issue().is_none());
    for delivery in &deliveries {
        delivery.cancel();
    }
    let resumed = stream
        .issue()
        .expect("cancelled input occupies no capacity");
    assert!(
        deliveries
            .iter()
            .all(|delivery| stream.cancelled(delivery.receipt()))
    );
    assert!(!stream.cancelled(resumed.receipt()));
    let mut progress = InputProgress::default();
    let command = progress.begin_command();
    progress.consumed(resumed.receipt());
    stream.observe(&progress.checkpoint());
    assert!(
        !stream.cancelled(deliveries[0].receipt()),
        "a frame that passed the run settles it"
    );
    drop(command);
}

/// Cancellations above an undelivered serial cannot be settled yet, but they
/// join the settled run once the gap below them is cancelled too.
#[test]
fn a_cancelled_gap_joins_the_cancellations_above_it() {
    let stream = stream(1);
    let gap = stream.issue().unwrap();
    let above: Vec<_> = (1..MAX_PENDING).map(|_| stream.issue().unwrap()).collect();
    for delivery in &above {
        delivery.cancel();
    }
    assert!(
        stream.issue().is_none(),
        "the oldest input is still in flight"
    );
    gap.cancel();
    assert!(stream.issue().is_some());
    assert!(
        above
            .iter()
            .all(|delivery| stream.cancelled(delivery.receipt()))
    );
}

#[test]
fn observed_frontier_forgets_settled_cancellations_only() {
    let stream = stream(1);
    let first = stream.issue().unwrap();
    let second = stream.issue().unwrap();
    first.cancel();
    second.cancel();
    let mut progress = InputProgress::default();
    stream.observe(&progress.checkpoint());
    assert!(stream.cancelled(first.receipt()) && stream.cancelled(second.receipt()));
    let later = stream.issue().unwrap().receipt();
    let command = progress.begin_command();
    progress.consumed(later);
    stream.observe(&progress.checkpoint());
    assert!(
        !stream.cancelled(first.receipt()) && !stream.cancelled(second.receipt()),
        "a consumed later serial settles every earlier one"
    );
    drop(command);
}

#[test]
fn serial_sets_decode_only_in_strictly_increasing_order() {
    let set = [3, 1, 2]
        .map(|raw| InputSerial::new(NonZeroU64::new(raw).unwrap()))
        .into_iter()
        .collect::<SerialSet>();
    assert_eq!(serde_json::to_string(&set).unwrap(), "[1,2,3]");
    assert!(serde_json::from_str::<SerialSet>("[1,3,3]").is_err());
    assert!(serde_json::from_str::<SerialSet>("[2,1]").is_err());
    assert_eq!(serde_json::from_str::<SerialSet>("[1,2,3]").unwrap(), set);
}

#[test]
fn layout_that_consumed_input_settles_indeterminate() {
    let stream = stream(1);
    let mut progress = InputProgress::default();
    let earlier = stream.issue().unwrap().receipt();
    progress.consumed(earlier);
    let command = progress.begin_command();
    let before = progress.checkpoint();
    drop(command);
    let completed_meanwhile = FrontierSnapshot::settle(before.clone(), &progress.checkpoint());
    assert_eq!(
        completed_meanwhile, before,
        "completion alone keeps the pre-layout frontier"
    );
    let nested = stream.issue().unwrap().receipt();
    progress.consumed(nested);
    let consumed_meanwhile = FrontierSnapshot::settle(before, &progress.checkpoint());
    assert!(matches!(
        consumed_meanwhile.as_slice(),
        [FrontierSnapshot::Indeterminate { stream: settled, .. }] if *settled == stream.id()
    ));
    assert_eq!(
        Disposition::of(earlier, &consumed_meanwhile),
        Disposition::Pending,
        "its command was still running when layout began"
    );
    assert_eq!(
        Disposition::of(nested, &consumed_meanwhile),
        Disposition::Pending,
        "input consumed during layout is not knowable from the frame"
    );
    let other = self::stream(2);
    let mut fresh = InputProgress::default();
    let first_seen = fresh.checkpoint();
    fresh.consumed(other.issue().unwrap().receipt());
    let first_consumed_meanwhile = FrontierSnapshot::settle(first_seen, &fresh.checkpoint());
    assert!(matches!(
        first_consumed_meanwhile.as_slice(),
        [FrontierSnapshot::Indeterminate { stream: settled, .. }] if *settled == other.id()
    ));
}

/// The compat review's scenario: layout Lisp reads an event, so the frame's
/// stream is indeterminate, yet an earlier input whose command completed
/// before layout began is fully shown by the frame. The pre-value checkpoint
/// (captured before layout) acknowledged it; the settled frontier must too,
/// or the pixel-scroll reconciler applies that input's offset a second time.
#[test]
fn an_indeterminate_frame_still_reflects_what_completed_before_layout() {
    let stream = stream(1);
    let mut progress = InputProgress::default();
    let completed = stream.issue().unwrap().receipt();
    {
        let _command = progress.begin_command();
        progress.consumed(completed);
    }
    let running = stream.issue().unwrap().receipt();
    let outer = progress.begin_command();
    progress.consumed(running);
    let before = progress.checkpoint();
    let read_by_layout = stream.issue().unwrap().receipt();
    progress.consumed(read_by_layout);
    let frame = FrontierSnapshot::settle(before, &progress.checkpoint());
    assert_eq!(Disposition::of(completed, &frame), Disposition::Reflected);
    assert_eq!(Disposition::of(running, &frame), Disposition::Pending);
    assert_eq!(
        Disposition::of(read_by_layout, &frame),
        Disposition::Pending
    );
    drop(outer);
}

/// DESIGN §6.14: the frontier a frame carries must be exact for a nested
/// command reader. Command scopes open before keys are read, so an outer
/// command (`M-x`) stays in progress while the minibuffer's nested command
/// loop completes one command per inner key.
mod frontier {
    use super::*;

    #[test]
    fn minibuffer_inner_keys_reflected_while_outer_in_progress() {
        let stream = stream(1);
        let outer = stream.issue().unwrap().receipt();
        let inner_keys = [
            stream.issue().unwrap().receipt(),
            stream.issue().unwrap().receipt(),
        ];
        let mut progress = InputProgress::default();
        let outer_command = progress.begin_command();
        progress.consumed(outer);
        for inner in inner_keys {
            let inner_command = progress.begin_command();
            progress.consumed(inner);
            drop(inner_command);
        }
        let frame = progress.checkpoint();
        for inner in inner_keys {
            assert_eq!(
                Disposition::of(inner, &frame),
                Disposition::Reflected,
                "an inner key whose nested command completed is reflected by this frame"
            );
        }
        assert_eq!(
            Disposition::of(outer, &frame),
            Disposition::Partial,
            "the outer key's command is still running"
        );
        drop(outer_command);
    }

    /// Equivalence guard: for the single-event inputs that carry receipts
    /// (scroll, PageUp, PageDown), each consumed by its own command in order,
    /// `Reflected` is exactly the pre-value test `through >= serial`, where
    /// `through` advanced while the oldest issued input was completed or
    /// cancelled. Cancelled inputs are excluded: the compositor drops them by
    /// their cancellation, never by the frontier.
    #[test]
    fn frontier_single_event_inputs_match_completed_prefix() {
        let stream = stream(1);
        let mut progress = InputProgress::default();
        let mut consumed = Vec::new();
        let mut resolved = std::collections::BTreeSet::new();
        let check = |consumed: &[InputReceipt],
                     resolved: &std::collections::BTreeSet<u64>,
                     progress: &InputProgress| {
            let old_through = (1..)
                .take_while(|serial| resolved.contains(serial))
                .last()
                .unwrap_or(0);
            let frame = progress.checkpoint();
            for receipt in consumed {
                assert_eq!(
                    Disposition::of(*receipt, &frame) == Disposition::Reflected,
                    old_through >= receipt.serial().get(),
                    "serial {} against the old prefix {old_through}",
                    receipt.serial().get()
                );
            }
        };
        for step in 0..12 {
            let delivery = stream.issue().unwrap();
            check(&consumed, &resolved, &progress);
            if step % 4 == 3 {
                // Discarded before the evaluator's queue: never consumed.
                delivery.cancel();
                resolved.insert(delivery.receipt().serial().get());
                check(&consumed, &resolved, &progress);
                continue;
            }
            let command = progress.begin_command();
            progress.consumed(delivery.receipt());
            consumed.push(delivery.receipt());
            check(&consumed, &resolved, &progress);
            drop(command);
            resolved.insert(delivery.receipt().serial().get());
            check(&consumed, &resolved, &progress);
        }
    }
}
