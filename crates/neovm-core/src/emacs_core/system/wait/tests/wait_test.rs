use super::*;

#[test]
fn target_process_activity_implies_any_process_activity() {
    let mut process = ProcessOutputServiceOutcome::default();
    let mut outcome = WaitServiceOutcome::default();

    process.record_activity(true);
    outcome.absorb_process_activity(process);

    assert!(outcome.has_target_process_activity());
    assert!(outcome.has_any_process_activity());
}

#[test]
fn resize_special_input_activity_implies_any_special_input_activity() {
    let mut outcome = WaitServiceOutcome::default();

    outcome.absorb_special_input_activity(SpecialInputServiceOutcome::resize_with_redisplay());

    assert!(outcome.has_resize_activity());
    assert!(outcome.has_special_input_activity());
}

#[test]
fn special_input_outcome_constructs_resize_activity_explicitly() {
    let outcome = SpecialInputServiceOutcome::resize_with_redisplay();

    assert!(outcome.has_resize_activity());
    assert!(outcome.has_any_activity());
    assert!(outcome.redisplay_needed());
}

#[test]
fn special_input_outcome_merges_activity_and_redisplay() {
    let outcome = SpecialInputServiceOutcome::any_activity()
        .merge(SpecialInputServiceOutcome::resize_with_redisplay());

    assert!(outcome.has_resize_activity());
    assert!(outcome.has_any_activity());
    assert!(outcome.redisplay_needed());
}

#[test]
fn command_input_pending_is_recorded_explicitly() {
    let mut outcome = WaitServiceOutcome::default();

    outcome.record_command_input_pending();

    assert!(outcome.has_command_input_pending());
}

#[test]
fn timer_activity_is_recorded_explicitly() {
    let mut outcome = WaitServiceOutcome::default();

    outcome.record_timer_activity(true);

    assert!(outcome.has_timer_activity());
}

fn gnu_timer_vector_at(deadline: GnuTimerTimestamp) -> crate::emacs_core::value::Value {
    use crate::emacs_core::value::Value;

    Value::vector(vec![
        Value::NIL,
        Value::fixnum(deadline.high_seconds),
        Value::fixnum(deadline.low_seconds),
        Value::fixnum(deadline.usecs),
        Value::NIL,
        Value::symbol("ignore"),
        Value::NIL,
        Value::NIL,
        Value::fixnum(deadline.psecs),
        Value::NIL,
    ])
}

#[test]
fn ordinary_timer_deadline_filter_excludes_boundary_timer() {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::value::Value;

    let now = GnuTimerTimestamp::now();
    let before_deadline = now.add_duration(Duration::from_millis(10));
    let deadline = now.add_duration(Duration::from_millis(20));
    let mut context = Context::new();

    context.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_vector_at(deadline)]),
    );
    assert_eq!(
        context.next_ordinary_gnu_timer_timeout_before(Some(deadline)),
        None
    );

    context.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_vector_at(before_deadline)]),
    );
    assert!(
        context
            .next_ordinary_gnu_timer_timeout_before(Some(deadline))
            .is_some()
    );
}

#[test]
fn source_events_construct_notification_wakeup_explicitly() {
    let events = ProcessWaitEvents::notification_wakeup();

    assert!(events.has_notification_wakeup());
    assert!(!events.has_ready_processes());
}

#[test]
fn source_events_construct_ready_processes_explicitly() {
    let events = ProcessWaitEvents::ready_processes(vec![7]);

    assert!(!events.has_notification_wakeup());
    assert!(events.has_ready_process(7));
}

#[test]
fn source_events_query_individual_ready_processes() {
    let events = ProcessWaitEvents::ready_processes(vec![7]);

    assert!(events.has_ready_process(7));
    assert!(!events.has_ready_process(8));
}

#[test]
fn source_events_empty_query_reflects_recorded_activity() {
    let empty = ProcessWaitEvents::default();
    let ready = ProcessWaitEvents::ready_processes(vec![7]);

    assert!(empty.is_empty());
    assert!(!ready.is_empty());
}

#[test]
fn source_events_convert_to_process_service() {
    let events = ProcessWaitEvents::ready_processes(vec![7]);
    let activity = WaitBlockActivity::from_source_events(events);

    assert_eq!(
        activity.into_process_service(),
        WaitProcessService::Ready(ProcessWaitEvents::ready_processes(vec![7]))
    );
}

#[test]
fn empty_source_events_poll_all_processes() {
    let activity = WaitBlockActivity::from_source_events(ProcessWaitEvents::default());

    assert!(!activity.has_notification_wakeup());
    assert_eq!(activity.into_process_service(), WaitProcessService::Poll);
}

#[test]
fn notification_only_source_events_poll_all_processes() {
    let activity = WaitBlockActivity::from_source_events(ProcessWaitEvents::notification_wakeup());

    assert!(activity.has_notification_wakeup());
    assert_eq!(activity.into_process_service(), WaitProcessService::Poll);
}

#[test]
fn block_activity_from_source_events_preserves_wakeup_and_processes() {
    let events = ProcessWaitEvents::from_sources(true, vec![3]);

    let activity = WaitBlockActivity::from_source_events(events);

    assert!(activity.has_notification_wakeup());
    assert_eq!(
        activity.into_process_service(),
        WaitProcessService::Ready(ProcessWaitEvents::from_sources(true, vec![3]))
    );
}

#[test]
fn block_activity_from_ready_processes_has_no_notification_wakeup() {
    let activity = WaitBlockActivity::ready_processes(vec![4, 9]);

    assert!(!activity.has_notification_wakeup());
    assert!(activity.has_external_activity());
    assert_eq!(
        activity.into_process_service(),
        WaitProcessService::Ready(ProcessWaitEvents::ready_processes(vec![4, 9]))
    );
}

#[test]
fn context_services_source_events_directly() {
    let mut context = crate::emacs_core::eval::Context::new();
    let request = WaitRequest::service_once(false);

    let outcome = context
        .service_wait_request_source_events_outcome(&request, ProcessWaitEvents::default())
        .expect("service source events");

    assert!(!outcome.has_command_input_pending());
    assert!(!outcome.has_any_process_activity());
}

#[test]
fn block_for_wait_request_zero_timeout_returns_poll_activity() {
    let mut context = crate::emacs_core::eval::Context::new();
    let request = WaitRequest::service_once(false);

    let activity = context
        .block_for_wait_request(&request, Duration::ZERO)
        .expect("block for wait request");

    assert!(!activity.has_notification_wakeup());
    assert_eq!(activity.into_process_service(), WaitProcessService::Poll);
}

#[test]
fn deadline_timeout_without_activity_suppresses_timers_after_block() {
    let timeout = WaitTimeoutChoice {
        duration: Duration::from_millis(20),
        finite_deadline_timeout: true,
        shortened_by_timer: false,
    };
    let activity = WaitBlockActivity::poll();

    assert!(!timeout.run_timers_after_block(&activity, false));
}

#[test]
fn timer_shortened_timeout_runs_timers_after_block_before_deadline() {
    let timeout = WaitTimeoutChoice {
        duration: Duration::from_millis(1),
        finite_deadline_timeout: false,
        shortened_by_timer: true,
    };
    let activity = WaitBlockActivity::poll();

    assert!(timeout.run_timers_after_block(&activity, false));
    assert!(!timeout.run_timers_after_block(&activity, true));
}

#[test]
fn external_activity_before_deadline_allows_timers_after_block() {
    let timeout = WaitTimeoutChoice {
        duration: Duration::from_millis(20),
        finite_deadline_timeout: true,
        shortened_by_timer: false,
    };
    let activity = WaitBlockActivity::ready_processes(vec![1]);

    assert!(timeout.run_timers_after_block(&activity, false));
}

#[test]
fn wait_request_exposes_deadline_and_process_completion_queries() {
    let request = WaitRequest::accept_target_process_output_with_timers(
        ProcessOutputWaitTiming::Poll,
        12,
        false,
    );

    assert_eq!(request.deadline(), WaitDeadline::Poll);
    assert_eq!(request.target_process(), Some(12));
    assert!(request.completes_on_target_process_activity(12));
    assert!(!request.completes_on_any_process_activity());
    assert!(!request.restricts_process_service_to_target());
}

#[test]
fn wait_request_accept_process_output_constructors_capture_timer_policy() {
    let run = WaitRequest::accept_any_process_output_with_timers(ProcessOutputWaitTiming::Poll);
    let suppress =
        WaitRequest::accept_any_process_output_without_timers(ProcessOutputWaitTiming::Poll);

    assert!(run.runs_timers());
    assert!(!suppress.runs_timers());
}

#[test]
fn wait_request_accept_process_output_named_constructors_capture_process_scope() {
    let any = WaitRequest::accept_any_process_output_with_timers(ProcessOutputWaitTiming::Poll);
    let target = WaitRequest::accept_target_process_output_with_timers(
        ProcessOutputWaitTiming::Poll,
        7,
        false,
    );
    let target_only = WaitRequest::accept_target_process_output_without_timers(
        ProcessOutputWaitTiming::Forever,
        9,
        true,
    );

    assert!(any.completes_on_any_process_activity());
    assert_eq!(any.target_process(), None);
    assert!(target.completes_on_target_process_activity(7));
    assert!(!target.restricts_process_service_to_target());
    assert!(target_only.completes_on_target_process_activity(9));
    assert!(target_only.restricts_process_service_to_target());
    assert!(!target_only.runs_timers());
    assert!(target_only.deadline_is_forever());
}

#[test]
fn wait_request_process_output_timing_converts_duration_to_finite_deadline() {
    let request = WaitRequest::accept_any_process_output_with_timers(ProcessOutputWaitTiming::For(
        Duration::from_millis(5),
    ));

    assert!(request.deadline_is_finite());
}

#[test]
fn wait_request_timer_service_suppresses_special_input_and_processes() {
    let request = WaitRequest::timer_service(true);

    assert_eq!(request.deadline(), WaitDeadline::Poll);
    assert_eq!(request.target_process(), None);
    assert!(!request.completes_on_any_process_activity());
    assert!(!request.services_special_input());
}

#[test]
fn wait_request_input_pending_constructors_capture_timer_policy() {
    let suppress = WaitRequest::input_pending_without_timers();
    let run = WaitRequest::input_pending_with_timers();

    assert!(!suppress.runs_timers());
    assert!(run.runs_timers());
}

#[test]
fn wait_request_exposes_scheduler_queries() {
    let now = Instant::now();
    let read = WaitRequest::read_command_input_until(now + Duration::from_secs(1));
    let poll = WaitRequest::service_once(true);
    let resize = WaitRequest::resize_ack(now);

    assert!(read.waits_for_host_input());
    assert!(read.completes_on_command_input());
    assert!(read.sets_waiting_for_user_input());
    assert!(read.runs_timers());
    assert!(!read.poll_or_deadline_elapsed(now));
    assert_eq!(read.base_timeout(now), Duration::from_secs(1));
    assert_eq!(
        read.base_timeout(now + Duration::from_secs(2)),
        Duration::ZERO
    );

    assert!(!poll.waits_for_host_input());
    assert!(!poll.completes_on_command_input());
    assert!(poll.poll_or_deadline_elapsed(now));

    assert!(resize.waits_for_host_input());
    assert!(!resize.runs_timers());
}

#[test]
fn wait_request_redisplay_query_tracks_request_and_activity() {
    let redisplay = WaitRequest::service_once(true);
    let quiet = WaitRequest::service_once(false);
    let mut special = SpecialInputServiceOutcome::default();
    let mut service = WaitServiceOutcome::default();

    assert!(!redisplay.needs_redisplay_after_service(special, service));

    special = SpecialInputServiceOutcome::resize_with_redisplay();
    assert!(redisplay.needs_redisplay_after_service(special, service));
    assert!(!quiet.needs_redisplay_after_service(special, service));

    special = SpecialInputServiceOutcome::from_internal_effects(
        crate::frontend_events::InternalEventEffects {
            redisplay_needed: true,
        },
    );
    assert!(
        redisplay.needs_redisplay_after_service(special, service),
        "a late frontend report must repaint even when it is the only idle-wait activity"
    );

    special = SpecialInputServiceOutcome::default();
    service.record_timer_activity(true);
    assert!(redisplay.needs_redisplay_after_service(special, service));

    // A process filter that read output bytes may have changed a displayed
    // buffer (comint/async-shell output): redisplay after service.
    let mut output = WaitServiceOutcome::default();
    let mut output_process = ProcessOutputServiceOutcome::default();
    output_process.record_activity(false);
    output.absorb_process_activity(output_process);
    assert!(redisplay.needs_redisplay_after_service(SpecialInputServiceOutcome::default(), output));
    // But output activity must NOT complete a command-input wait's redisplay
    // path via a quiet request.
    assert!(!quiet.needs_redisplay_after_service(SpecialInputServiceOutcome::default(), output));

    // A sentinel/EOF that ran no output read still repaints (eww's
    // completion callback runs in url-http's EOF sentinel): the `serviced`
    // bit alone drives redisplay, even though it must never complete an
    // `accept-process-output` wait.
    let mut serviced = WaitServiceOutcome::default();
    let mut serviced_process = ProcessOutputServiceOutcome::default();
    serviced_process.record_serviced();
    serviced.absorb_process_activity(serviced_process);
    assert!(!serviced.has_any_process_activity());
    assert!(serviced.ran_process_callbacks());
    assert!(
        redisplay.needs_redisplay_after_service(SpecialInputServiceOutcome::default(), serviced)
    );
}
