fn test_ob() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}
use super::*;
use crate::emacs_core::eval::Context;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn eval_first_form_after_marker(eval: &mut Context, source: &str, marker: &str) {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("missing GNU subr.el marker: {marker}"));
    let (form, _) = crate::emacs_core::value_reader::read_one(&source[start..], 0, &test_ob())
        .unwrap_or_else(|err| panic!("parse GNU subr.el from {marker} failed: {:?}", err))
        .unwrap_or_else(|| panic!("no GNU subr.el form found after marker: {marker}"));
    eval.eval_form(form)
        .unwrap_or_else(|err| panic!("evaluate GNU subr.el form {marker} failed: {:?}", err));
}

/// Install minimal `defun`/`defmacro`/`when`/`unless` shims so a bare
/// evaluator can evaluate forms extracted from GNU `.el` source files.
fn install_bare_elisp_shims(ev: &mut Context) {
    let shims = r#"
(defalias 'defun (cons 'macro #'(lambda (name arglist &rest body)
  (list 'defalias (list 'quote name) (cons 'function (list (cons 'lambda (cons arglist body))))))))
(defalias 'defmacro (cons 'macro #'(lambda (name arglist &rest body)
  (list 'defalias (list 'quote name)
        (list 'cons ''macro (cons 'function (list (cons 'lambda (cons arglist body)))))))))
(defalias 'when (cons 'macro #'(lambda (cond &rest body)
  (list 'if cond (cons 'progn body)))))
(defalias 'unless (cons 'macro #'(lambda (cond &rest body)
  (cons 'if (cons cond (cons nil body))))))
"#;
    ev.eval_str(shims).expect("install bare elisp shims");
}

fn gnu_subr_sit_for_eval() -> Context {
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let subr_path = project_root.join("lisp/subr.el");
    let subr_source = fs::read_to_string(&subr_path).expect("read GNU subr.el");

    let mut ev = Context::new();
    install_bare_elisp_shims(&mut ev);
    ev.set_lexical_binding(true);
    eval_first_form_after_marker(
        &mut ev,
        &subr_source,
        "(defun sit-for (seconds &optional nodisp)",
    );
    ev
}

fn install_minimal_special_event_command_runtime(ev: &mut Context) {
    ev.eval_str(
        r#"
(fset 'command-execute
      (lambda (cmd &optional _record keys _special)
        (funcall cmd (aref keys 0))))
(fset 'handle-delete-frame
      (lambda (event)
        (setq neo-last-delete-frame-event event)
        nil))
"#,
    )
    .expect("install special-event command runtime");
}

fn gnu_timer_before(delay: Duration, callback: &str) -> Value {
    let when = SystemTime::now()
        .checked_sub(delay)
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .expect("timer deadline should not precede unix epoch");
    let secs = when.as_secs() as i64;

    Value::vector(vec![
        Value::NIL,
        Value::fixnum(secs >> 16),
        Value::fixnum(secs & 0xFFFF),
        Value::fixnum(when.subsec_micros() as i64),
        Value::NIL,
        Value::symbol(callback),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
    ])
}

#[test]
fn gnu_sit_for_matches_subr_el() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_subr_sit_for_eval();
    let first = ev
        .eval_str("(let ((noninteractive t)) (sit-for 0.0))")
        .expect("eval sit-for");
    assert!(first.is_truthy());

    let second = ev
        .eval_str("(let ((noninteractive t)) (sit-for 0.01 t))")
        .expect("eval sit-for nodisp");
    assert!(second.is_truthy());
}

#[test]
fn gnu_sit_for_interactive_timeout_returns_t() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_subr_sit_for_eval();
    ev.set_variable("noninteractive", Value::NIL);
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let start = Instant::now();
    let result = ev
        .eval_str("(sit-for 0.01 t)")
        .expect("eval interactive sit-for");
    drop(tx);

    assert!(result.is_truthy());
    assert!(start.elapsed() < Duration::from_millis(250));
}

#[test]
fn gnu_sit_for_with_pending_input_does_not_run_timers_first() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_subr_sit_for_eval();
    ev.set_variable("noninteractive", Value::NIL);
    ev.eval_str(
        r#"(progn
             (setq sit-for-pending-input-timer-fired nil)
             (fset 'sit-for-pending-input-timer-callback
                   (lambda ()
                     (setq sit-for-pending-input-timer-fired 'done))))"#,
    )
    .expect("install sit-for pending-input timer setup");
    ev.eval_str(
        r#"(fset 'timer-event-handler
                 (lambda (timer)
                   (setq timer-list (delq timer timer-list))
                   (apply (aref timer 5) (aref timer 6))))"#,
    )
    .expect("install timer-event-handler stub");
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(1),
            "sit-for-pending-input-timer-callback",
        )]),
    );

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .expect("queue keypress");
    ev.input_rx = Some(rx);
    let result = ev
        .eval_str("(sit-for 0.5 t)")
        .expect("eval interactive sit-for");

    assert!(result.is_nil());
    assert!(
        ev.eval_symbol("sit-for-pending-input-timer-fired")
            .expect("timer callback flag")
            .is_nil()
    );
    let event = ev.read_char().expect("keypress should remain available");
    assert_eq!(event, Value::fixnum('a' as i64));
}

#[test]
fn gnu_sit_for_pending_input_returns_nil_without_redisplay() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_subr_sit_for_eval();
    ev.set_variable("noninteractive", Value::NIL);
    let redisplays = Rc::new(RefCell::new(0usize));
    let redisplays_in_cb = Rc::clone(&redisplays);
    ev.redisplay_fn = Some(Box::new(move |_ev: &mut Context| {
        *redisplays_in_cb.borrow_mut() += 1;
    }));

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .expect("queue keypress");
    ev.input_rx = Some(rx);
    let result = ev
        .eval_str("(sit-for 0.5)")
        .expect("eval interactive sit-for");

    assert!(result.is_nil());
    assert_eq!(*redisplays.borrow(), 0);
    let event = ev.read_char().expect("keypress should remain available");
    assert_eq!(event, Value::fixnum('a' as i64));
}

#[test]
fn gnu_sit_for_zero_without_nodisp_redisplays_once() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_subr_sit_for_eval();
    ev.set_variable("noninteractive", Value::NIL);
    let redisplays = Rc::new(RefCell::new(0usize));
    let redisplays_in_cb = Rc::clone(&redisplays);
    ev.redisplay_fn = Some(Box::new(move |_ev: &mut Context| {
        *redisplays_in_cb.borrow_mut() += 1;
    }));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let result = ev
        .eval_str("(sit-for 0)")
        .expect("eval zero-second sit-for");
    drop(tx);

    assert!(result.is_truthy());
    assert_eq!(*redisplays.borrow(), 1);
}

#[test]
fn gnu_sit_for_zero_nodisp_runs_due_gnu_timer_without_redisplay() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_subr_sit_for_eval();
    ev.set_variable("noninteractive", Value::NIL);
    ev.eval_str(
        r#"(progn
             (setq sit-for-zero-timer-fired nil)
             (fset 'sit-for-zero-timer-callback
                   (lambda ()
                     (setq sit-for-zero-timer-fired 'done)))
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (funcall (aref timer 5)))))"#,
    )
    .expect("install zero-second sit-for timer setup");
    ev.set_variable(
        "timer-list",
        Value::list(vec![gnu_timer_before(
            Duration::from_millis(1),
            "sit-for-zero-timer-callback",
        )]),
    );

    let redisplays = Rc::new(RefCell::new(0usize));
    let redisplays_in_cb = Rc::clone(&redisplays);
    ev.redisplay_fn = Some(Box::new(move |_ev: &mut Context| {
        *redisplays_in_cb.borrow_mut() += 1;
    }));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let result = ev
        .eval_str("(sit-for 0 t)")
        .expect("eval zero-second sit-for");
    drop(tx);

    assert!(result.is_truthy());
    assert_eq!(*redisplays.borrow(), 0);
    assert_eq!(
        ev.eval_symbol("sit-for-zero-timer-fired")
            .expect("zero-second sit-for timer flag"),
        Value::symbol("done")
    );
}

#[test]
fn test_builtin_sleep_for() {
    crate::test_utils::init_test_tracing();
    use super::super::eval::Context;

    let mut eval = Context::new();

    let result = builtin_sleep_for(&mut eval, vec![Value::fixnum(0)]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());

    let result = builtin_sleep_for(&mut eval, vec![Value::fixnum(0), Value::fixnum(0)]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());

    let result = builtin_sleep_for(&mut eval, vec![]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));

    let result = builtin_sleep_for(
        &mut eval,
        vec![Value::fixnum(0), Value::fixnum(0), Value::fixnum(0)],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));

    let result = builtin_sleep_for(&mut eval, vec![Value::string("1")]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data == vec![Value::symbol("numberp"), Value::string("1")]
    ));

    let result = builtin_sleep_for(&mut eval, vec![Value::fixnum(0), Value::make_float(0.5)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-type-argument"
                && sig.data == vec![Value::symbol("fixnump"), Value::make_float(0.5)]
    ));
}

#[test]
fn sleep_for_window_close_uses_special_event_map_handler_when_loaded() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(scratch);
    let frame = ev.frames.create_frame("F1", 80, 24, scratch);
    install_minimal_special_event_command_runtime(&mut ev);

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::WindowClose {
        emacs_frame_id: frame.0,
    })
    .expect("queue window close");
    ev.input_rx = Some(rx);
    ev.command_loop.running = true;

    let result = builtin_sleep_for(&mut ev, vec![Value::make_float(0.01)])
        .expect("sleep-for should consume handled window close");
    drop(tx);

    assert_eq!(result, Value::NIL);
    let logged = ev
        .eval_symbol("neo-last-delete-frame-event")
        .expect("delete-frame event should be logged");
    assert_eq!(
        logged,
        Value::list(vec![
            Value::symbol("delete-frame"),
            Value::list(vec![Value::make_frame(frame.0)]),
        ]),
    );
}

#[test]
fn sleep_for_window_close_honors_throw_on_input_before_handler() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(scratch);
    let frame = ev.frames.create_frame("F1", 80, 24, scratch);
    install_minimal_special_event_command_runtime(&mut ev);

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::WindowClose {
        emacs_frame_id: frame.0,
    })
    .expect("queue window close");
    ev.input_rx = Some(rx);
    ev.command_loop.running = true;
    ev.obarray
        .set_symbol_value("throw-on-input", Value::symbol("tag"));

    let flow = builtin_sleep_for(&mut ev, vec![Value::make_float(0.01)])
        .expect_err("throw-on-input should interrupt sleep-for");
    assert!(matches!(
        flow,
        Flow::Throw(ref thrown)
            if thrown.tag == Value::symbol("tag") && thrown.value == Value::T
    ));

    ev.obarray.set_symbol_value("throw-on-input", Value::NIL);
    let result = builtin_sleep_for(&mut ev, vec![Value::make_float(0.01)])
        .expect("sleep-for should consume handled window close afterwards");
    drop(tx);

    assert_eq!(result, Value::NIL);
    let logged = ev
        .eval_symbol("neo-last-delete-frame-event")
        .expect("delete-frame event should be logged");
    assert_eq!(
        logged,
        Value::list(vec![
            Value::symbol("delete-frame"),
            Value::list(vec![Value::make_frame(frame.0)]),
        ]),
    );
}
