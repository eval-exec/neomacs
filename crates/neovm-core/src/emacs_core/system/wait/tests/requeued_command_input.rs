use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn timer_requeued_command_input_completes_the_first_service_pass() {
    let _policy = crate::emacs_core::eval::RedisplayHookPolicyGuard::legacy();
    let mut context = Context::new();
    context
        .eval_str(
            "(fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list nil unread-command-events '(?x))))",
        )
        .expect("install a timer which queues command input");
    context.set_variable(
        "timer-list",
        Value::list(vec![Value::from(GnuTimerTimestamp::now())]),
    );

    let repaints = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&repaints);
    context.redisplay_fn = Some(Box::new(move |context| {
        observed
            .borrow_mut()
            .push(context.peek_unread_command_event());
    }));

    let outcome = context
        .service_wait_request_first_pass(&WaitRequest::read_command_input_until(
            Instant::now() + Duration::from_secs(10),
        ))
        .expect("service the ripe timer");

    assert!(outcome.has_timer_activity());
    assert_eq!(
        WaitRequest::read_command_input_forever().completion_for(outcome),
        Some(WaitCompletion::CommandInputPending)
    );
    assert_eq!(
        context.peek_unread_command_event(),
        Some(Value::fixnum(120))
    );
    assert_eq!(*repaints.borrow(), vec![Some(Value::fixnum(120))]);
}

#[test]
fn redisplay_consumed_input_does_not_leave_stale_command_completion() {
    let _policy = crate::emacs_core::eval::RedisplayHookPolicyGuard::legacy();
    let mut context = Context::new();
    context
        .eval_str(
            "(fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list nil unread-command-events '(?x))))",
        )
        .expect("install a timer which queues command input");
    context.set_variable(
        "timer-list",
        Value::list(vec![Value::from(GnuTimerTimestamp::now())]),
    );
    context.redisplay_fn = Some(Box::new(|context| {
        assert_eq!(context.pop_unread_command_event(), Some(Value::fixnum(120)));
    }));
    let request = WaitRequest::read_command_input_forever();

    let outcome = context
        .service_wait_request_first_pass(&request)
        .expect("repaint may consume requeued input");

    assert_eq!(
        request.completion_for(outcome),
        Some(WaitCompletion::DisplayActivity)
    );
    assert_eq!(context.peek_unread_command_event(), None);
}

#[test]
fn requeued_command_input_after_select_completes_without_running_timers() {
    let mut context = Context::new();
    // A cons whose first event is nil still meets GNU's CONSP predicate.
    context.set_variable("unread-command-events", Value::list(vec![Value::NIL]));
    let request = WaitRequest::read_command_input_until(Instant::now() + Duration::from_secs(10));

    let outcome = context
        .service_wait_request_block_activity(&request, WaitBlockActivity::poll(), false)
        .expect("service the select result without a timer pass");

    assert_eq!(
        request.completion_for(outcome),
        Some(WaitCompletion::CommandInputPending)
    );
    assert!(!outcome.has_timer_activity());
    assert_eq!(context.peek_unread_command_event(), Some(Value::NIL));

    context
        .eval_str("(fset 'timer-event-handler (lambda (timer) (error \"input must win\")))")
        .expect("install a timer which must remain pending");
    context.set_variable(
        "timer-list",
        Value::list(vec![Value::from(GnuTimerTimestamp::now())]),
    );
    let outcome = context
        .service_wait_request_block_activity(&request, WaitBlockActivity::poll(), true)
        .expect("requeued input must win even when the next timer pass is permitted");
    assert_eq!(
        request.completion_for(outcome),
        Some(WaitCompletion::CommandInputPending)
    );
    assert!(!outcome.has_timer_activity());
    assert_eq!(context.peek_unread_command_event(), Some(Value::NIL));
}

#[test]
fn requeued_command_input_does_not_complete_non_command_waits() {
    let mut context = Context::new();
    context.set_variable(
        "unread-command-events",
        Value::list(vec![Value::fixnum(120)]),
    );
    let requests = [
        WaitRequest::sleep_until(Instant::now() + Duration::from_secs(10)),
        WaitRequest::accept_any_process_output_with_timers(ProcessOutputWaitTiming::Poll),
        WaitRequest::timer_service(false),
    ];

    for request in requests {
        let outcome = context
            .service_wait_request_first_pass(&request)
            .expect("service a non-command request");
        assert!(!outcome.has_command_input_pending());
        let outcome = context
            .service_wait_request_block_activity(&request, WaitBlockActivity::poll(), false)
            .expect("service a non-command select result");
        assert!(!outcome.has_command_input_pending());
    }
    assert_eq!(
        context.peek_unread_command_event(),
        Some(Value::fixnum(120))
    );
}

#[test]
fn non_cons_and_input_method_queues_do_not_count_as_requeued_command_input() {
    let mut context = Context::new();
    let request = WaitRequest::read_command_input_until(Instant::now() + Duration::from_secs(10));
    for symbol in [
        "unread-input-method-events",
        "unread-post-input-method-events",
    ] {
        context.set_variable(symbol, Value::list(vec![Value::fixnum(120)]));
    }
    for value in [
        Value::NIL,
        Value::T,
        Value::fixnum(120),
        Value::vector(vec![]),
    ] {
        context.set_variable("unread-command-events", value);
        let outcome = context
            .service_wait_request_block_activity(&request, WaitBlockActivity::poll(), false)
            .expect("service a select result");
        assert!(!outcome.has_command_input_pending());
    }
}
