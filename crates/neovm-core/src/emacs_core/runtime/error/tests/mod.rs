use super::{EvalError, Flow, PrintShorthandSymbol, format_flow_with_eval, quote_payload, signal};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::{Context, Value, print_value_bytes_with_eval, print_value_with_eval};

#[test]
fn list_prints_buffers_with_names_in_eval_context() -> Result<(), EvalError> {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let stale = Value::make_buffer(eval.buffers.create_buffer("stale-win-buf"));
    eval.set_variable("vm-stale-win-buf", stale);
    let value = eval.eval_str(
        "(let ((b vm-stale-win-buf)
           (w (selected-window)))
  (set-window-buffer nil b)
  (kill-buffer b)
  (list (window-buffer) (window-start) (window-point)))",
    )?;

    assert_eq!(
        print_value_with_eval(&eval, &value),
        "(#<buffer *scratch*> 1 1)"
    );
    assert_eq!(
        String::from_utf8(print_value_bytes_with_eval(&eval, &value)).unwrap(),
        "(#<buffer *scratch*> 1 1)"
    );

    Ok(())
}

#[test]
fn eval_context_printer_renders_killed_buffer_handles() -> Result<(), EvalError> {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let value = eval.eval_str(
        "(with-temp-buffer
           (condition-case err
               (key-binding 1 nil nil 0)
             (error err)))",
    )?;

    assert_eq!(
        print_value_with_eval(&eval, &value),
        "(args-out-of-range #<killed buffer> 0)"
    );
    assert_eq!(
        String::from_utf8(print_value_bytes_with_eval(&eval, &value)).unwrap(),
        "(args-out-of-range #<killed buffer> 0)"
    );

    Ok(())
}

#[test]
fn diagnostic_flow_formatter_renders_signal_strings() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();
    let flow = signal(
        LispCondition::FileMissing,
        vec![
            Value::string("Cannot open load file"),
            Value::string("No such file or directory"),
            Value::string("popweb"),
        ],
    );

    assert_eq!(
        format_flow_with_eval(&eval, &flow),
        r#"(file-missing ("Cannot open load file" "No such file or directory" "popweb"))"#
    );
}

#[test]
fn eval_context_printer_renders_mutex_handles_consistently() -> Result<(), EvalError> {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let value = eval.eval_str(r#"(make-mutex "error-printer-mutex")"#)?;
    let printed = print_value_with_eval(&eval, &value);

    assert!(printed.starts_with("#<mutex "));
    assert_eq!(
        String::from_utf8(print_value_bytes_with_eval(&eval, &value)).unwrap(),
        printed
    );

    Ok(())
}

#[test]
fn eval_context_printer_renders_condvar_handles_consistently() -> Result<(), EvalError> {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let value = eval.eval_str(
        r#"(let ((m (make-mutex "error-printer-mutex")))
           (make-condition-variable m "error-printer-condvar"))"#,
    )?;
    let printed = print_value_with_eval(&eval, &value);

    assert!(printed.starts_with("#<condvar "));
    assert_eq!(
        String::from_utf8(print_value_bytes_with_eval(&eval, &value)).unwrap(),
        printed
    );

    Ok(())
}

#[test]
fn eval_context_printer_renders_frame_window_handles_consistently() -> Result<(), EvalError> {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let value = eval.eval_str("(list (selected-frame) (selected-window))")?;
    let printed = print_value_with_eval(&eval, &value);

    assert!(printed.starts_with("(#<frame"));
    assert!(printed.contains("#<window"));
    assert_eq!(
        String::from_utf8(print_value_bytes_with_eval(&eval, &value)).unwrap(),
        printed
    );

    Ok(())
}

#[test]
fn eval_context_printer_renders_window_handles_with_buffer_names() -> Result<(), EvalError> {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let value = eval.eval_str(
        "(list (selected-window)
               (condition-case err (frame-terminal (selected-window)) (error err))
               (condition-case err (tty-type (selected-window)) (error err))
               (condition-case err (terminal-name (selected-window)) (error err)))",
    )?;
    let printed = print_value_with_eval(&eval, &value);

    assert!(printed.contains("on *scratch*>"));
    assert_eq!(
        String::from_utf8(print_value_bytes_with_eval(&eval, &value)).unwrap(),
        printed
    );

    Ok(())
}

#[test]
fn eval_context_printer_renders_terminal_thread_handles_consistently() -> Result<(), EvalError> {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let value = eval.eval_str("(list (car (terminal-list)) (current-thread))")?;
    let printed = print_value_with_eval(&eval, &value);

    assert!(printed.starts_with("(#<terminal"));
    assert!(printed.contains("#<thread"));
    assert_eq!(
        String::from_utf8(print_value_bytes_with_eval(&eval, &value)).unwrap(),
        printed
    );

    Ok(())
}

#[test]
fn print_shorthand_symbol_domain_matches_gnu_printer_symbols() {
    crate::test_utils::init_test_tracing();
    for (symbol, name) in [
        (PrintShorthandSymbol::Quote, "quote"),
        (PrintShorthandSymbol::Function, "function"),
        (PrintShorthandSymbol::Backquote, "`"),
        (PrintShorthandSymbol::Comma, ","),
        (PrintShorthandSymbol::CommaAt, ",@"),
    ] {
        let value = Value::symbol(name);
        assert_eq!(symbol.name(), name);
        assert_eq!(PrintShorthandSymbol::from_lisp_value(&value), Some(symbol));
    }

    let quoted = Value::list(vec![Value::symbol("quote"), Value::symbol("foo")]);
    let function_quoted = Value::list(vec![Value::symbol("function"), Value::symbol("foo")]);
    assert_eq!(quote_payload(&quoted), Some(Value::symbol("foo")));
    assert_eq!(quote_payload(&function_quoted), None);
}

#[test]
fn eval_context_printer_matches_gnu_backquote_shorthand_rules() {
    crate::test_utils::init_test_tracing();
    // GNU verified via:
    //   (prin1-to-string (list '\` (list 'a (list '\, 'x))))
    //   => "`(a ,x)"
    // The reader-shorthand form is the canonical print of the
    // (` (a (, x))) form, *not* the verbatim escaped one.
    let eval = Context::new();
    let raw_unquote = Value::list(vec![Value::symbol(","), Value::symbol("x")]);
    let nested = Value::list(vec![
        Value::symbol("`"),
        Value::list(vec![Value::symbol("a"), raw_unquote]),
    ]);
    assert_eq!(print_value_with_eval(&eval, &nested), "`(a ,x)");
    assert_eq!(
        String::from_utf8(print_value_bytes_with_eval(&eval, &nested)).unwrap(),
        "`(a ,x)"
    );
}

#[test]
fn eval_context_printer_handles_default_circular_vector_backreference() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();
    let vector = Value::vector(vec![Value::NIL]);
    assert!(vector.set_vector_slot(0, vector));

    assert_eq!(print_value_with_eval(&eval, &vector), "[#0]");
    assert_eq!(print_value_bytes_with_eval(&eval, &vector), b"[#0]");
}

#[test]
fn eval_context_printer_handles_default_circular_cons_backreference() {
    crate::test_utils::init_test_tracing();
    let eval = Context::new();
    let cell = Value::cons(Value::NIL, Value::NIL);
    cell.set_cdr(cell);

    assert_eq!(print_value_with_eval(&eval, &cell), "(nil . #0)");
    assert_eq!(print_value_bytes_with_eval(&eval, &cell), b"(nil . #0)");
}

#[test]
fn minibuffer_quit_does_not_take_down_a_noninteractive_session() {
    crate::test_utils::init_test_tracing();
    // GNU's `command-error-default-function` guards its
    // stderr-then-kill-emacs branch with `!is_minibuffer_quit`
    // (keyboard.c:1064).  A plain `quit' still takes that branch.
    let mut eval = Context::new();
    eval.set_variable("noninteractive", Value::T);

    let minibuffer_quit = Value::list(vec![Value::symbol("minibuffer-quit")]);
    let reported = eval.command_error_default_report(minibuffer_quit, Value::string(""));
    assert!(
        reported.is_ok(),
        "aborting a minibuffer must not unwind the session: {reported:?}"
    );
    assert!(
        eval.shutdown_request().is_none(),
        "aborting a minibuffer must not request shutdown"
    );

    let plain_quit = Value::list(vec![Value::symbol("quit")]);
    let reported = eval.command_error_default_report(plain_quit, Value::string(""));
    assert!(
        matches!(reported, Err(Flow::Shutdown(_))),
        "a plain quit keeps GNU's stderr-then-exit behavior: {reported:?}"
    );
}

#[test]
fn raw_condition_inheriting_minibuffer_quit_does_not_take_down_session() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_variable("noninteractive", Value::T);
    let condition = crate::emacs_core::intern::intern_lisp_string(
        &crate::heap_types::LispString::from_unibyte(vec![0xff]),
    );
    eval.obarray
        .put_property_id(
            condition,
            crate::emacs_core::intern::intern("error-conditions"),
            Value::list(vec![
                Value::from_sym_id(condition),
                Value::symbol("minibuffer-quit"),
                Value::symbol("quit"),
            ]),
        )
        .expect("raw condition should accept an identity-keyed hierarchy");

    let reported = eval.command_error_default_report(
        Value::list(vec![Value::from_sym_id(condition)]),
        Value::string(""),
    );
    assert!(
        reported.is_ok(),
        "minibuffer-quit ancestry is matched by identity: {reported:?}"
    );
    assert!(eval.shutdown_request().is_none());
}

// ---------------------------------------------------------------------------
// In-flight signal payload rooting (DIVERGENCES.md 161)
// ---------------------------------------------------------------------------

/// A signal that is unwinding lives only in a Rust `Flow::Signal`. GNU gets
/// this for free — `signal_or_quit` carries the payload on the C stack and
/// `mark_stack` scans it conservatively — but this collector is precise, so
/// the payload has to be an explicit root or a collection reached from an
/// `unwind-protect` cleanup reclaims it and `condition-case` binds a dangling
/// cons.
///
/// This is the cheap, direct pin for that class: build a signal whose payload
/// is heap-allocated, drop every OTHER reference to it, collect, and require
/// the payload to still be a live list. Before the fix the cons was on the
/// free list, so its car read back as [`Value::DEAD`] — GNU's `dead_object`.
#[test]
fn in_flight_signal_payload_survives_a_collection() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let flow = crate::emacs_core::error::signal_with_data(
        LispCondition::Error,
        Value::list(vec![Value::string("Malformed argument list")]),
    );

    // Nothing else references the payload: it is reachable ONLY through the
    // in-flight signal, which is what the root set has to cover.
    eval.gc_collect();

    let Flow::Signal(sig) = &flow else {
        panic!("signal_with_data builds a signal flow");
    };
    let raw = sig.raw_data.expect("signal_with_data records raw data");
    assert!(raw.is_cons(), "payload stays a cons: {raw:?}");
    assert!(
        !raw.cons_car().is_dead(),
        "the in-flight signal payload was collected while the signal was still \
         unwinding (its cons is on the free list)"
    );
    assert_eq!(
        print_value_with_eval(&eval, &super::make_signal_binding_value(sig)),
        "(error \"Malformed argument list\")"
    );
}

/// The same guarantee one level up: what `condition-case` binds must be a live
/// datum after a collection, not a resurrected free-list cell. This is the
/// shape the oracle probe `div_v8_cl_defun_key_aux_rest_optional` hit — the
/// error datum printed as `(error . <garbage symbol>)` and the printer panicked
/// in `resolve_sym_lisp_string`.
#[test]
fn condition_case_binding_value_survives_a_collection() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let flow = crate::emacs_core::error::signal_with_data(
        LispCondition::Error,
        Value::list(vec![Value::string("Malformed argument list ends with")]),
    );
    eval.gc_collect();

    let Flow::Signal(sig) = &flow else {
        panic!("signal flow");
    };
    let bound = super::make_signal_binding_value(sig);
    assert_eq!(
        print_value_with_eval(&eval, &bound),
        "(error \"Malformed argument list ends with\")"
    );
}

// ---------------------------------------------------------------------------
// In-flight THROW and THREAD-BLOCKED payload rooting (DIVERGENCES.md 162)
// ---------------------------------------------------------------------------

/// `throw` unwinds through exactly the machinery `signal` does: every frame it
/// passes runs `unbind_to`, which executes `unwind-protect` cleanup forms and
/// variable watchers — arbitrary Lisp, and therefore allocation-bearing safe
/// points. GNU is safe here for free: `Fthrow` -> `unwind_to_catch`
/// (src/eval.c:1188-1226) stores the value in `catchlist->val` and `longjmp`s,
/// leaving it on the C stack, which `mark_stack` (src/alloc.c) scans
/// conservatively. This collector is PRECISE — `set_stack_bottom` is a no-op
/// (`tagged/CONCURRENT_GC.md`, "precise-rooting precondition") — so a
/// `Flow::Throw` payload that is not a seeded root is reclaimed mid-unwind and
/// `catch` returns a free-list cell.
///
/// Red before the pin: the payload cons's car reads back as [`Value::DEAD`],
/// GNU's `dead_object` (src/alloc.c:6858) that `set_free_next` now writes.
#[test]
fn in_flight_throw_payload_survives_a_collection() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let flow = Flow::throw(
        Value::symbol("neovm-throw-probe"),
        Value::list(vec![Value::string("thrown datum")]),
    );

    // Nothing else references the thrown datum: it is reachable ONLY through
    // the in-flight throw, which is what the root set has to cover.
    eval.gc_collect();

    let Flow::Throw(thrown) = &flow else {
        panic!("Flow::throw builds a throw flow");
    };
    assert!(
        thrown.value.is_cons(),
        "payload stays a cons: {:?}",
        thrown.value
    );
    assert!(
        !thrown.value.cons_car().is_dead(),
        "the in-flight throw payload was collected while the throw was still \
         unwinding (its cons is on the free list)"
    );
    assert_eq!(
        print_value_with_eval(&eval, &thrown.value),
        "(\"thrown datum\")"
    );
}

/// The cooperative thread-yield handoff carries two Lisp values up the same
/// Rust stack, and `sf_condition_case_value_named`'s `ThreadBlocked` arm
/// rebuilds a continuation from them — allocation, and one frame out the same
/// `unwind-protect` cleanups. Same class, same fix.
#[test]
fn in_flight_thread_blocked_payload_survives_a_collection() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let flow = Flow::thread_blocked(
        Value::list(vec![Value::string("blocker datum")]),
        Value::list(vec![Value::string("remaining form")]),
    );

    eval.gc_collect();

    let Flow::ThreadBlocked(blocked) = &flow else {
        panic!("Flow::thread_blocked builds a thread-blocked flow");
    };
    assert!(
        !blocked.blocker.cons_car().is_dead(),
        "the in-flight thread-blocked BLOCKER was collected while the yield \
         was still in flight"
    );
    assert!(
        !blocked.remaining_forms.cons_car().is_dead(),
        "the in-flight thread-blocked REMAINING-FORMS were collected while the \
         yield was still in flight"
    );
    assert_eq!(
        print_value_with_eval(&eval, &blocked.remaining_forms),
        "(\"remaining form\")"
    );
}

/// The boundary type has the same problem the flow did. `map_flow` converts a
/// `Flow::Signal` into the PUBLIC `EvalError::Signal`, and before this entry
/// that conversion moved `data`/`raw_data` out of the pinned `SignalData` and
/// dropped the pin — so any boundary holding an `EvalError` while more Lisp
/// runs (every `load_file` caller, the worker task boundary, the batch
/// `--eval` reporter) was holding values the collector could not see.
///
/// 161 recorded this as needing a shape change across 73 use sites. It does
/// not: an enum variant cannot have a private field, but it CAN have a field
/// whose type has no constructor outside the module, which makes the literal
/// unwritable and leaves every `{ symbol, data, .. }` pattern untouched.
#[test]
fn eval_error_signal_payload_survives_a_collection() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    // `map_flow` consumes the `Flow`, so the flow's own pin is gone by the
    // time this returns: only the `EvalError`'s pin can keep the payload.
    let err = crate::emacs_core::error::map_flow(crate::emacs_core::error::signal_with_data(
        LispCondition::Error,
        Value::list(vec![Value::string("boundary datum")]),
    ));

    eval.gc_collect();

    let EvalError::Signal { raw_data, .. } = &err else {
        panic!("map_flow of a signal is an EvalError::Signal");
    };
    let raw = raw_data.expect("signal_with_data records raw data");
    assert!(
        !raw.cons_car().is_dead(),
        "the EvalError payload was collected while the error was still in \
         flight at a boundary (its cons is on the free list)"
    );
    assert_eq!(print_value_with_eval(&eval, &raw), "(\"boundary datum\")");
}

/// The throw half of the same boundary.
#[test]
fn eval_error_uncaught_throw_payload_survives_a_collection() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let err = crate::emacs_core::error::map_flow(Flow::throw(
        Value::symbol("neovm-boundary-throw"),
        Value::list(vec![Value::string("boundary thrown")]),
    ));

    eval.gc_collect();

    let EvalError::UncaughtThrow { value, .. } = &err else {
        panic!("map_flow of a throw is an EvalError::UncaughtThrow");
    };
    assert!(
        !value.cons_car().is_dead(),
        "the EvalError uncaught-throw payload was collected while the error \
         was still in flight at a boundary"
    );
    assert_eq!(print_value_with_eval(&eval, value), "(\"boundary thrown\")");
}
