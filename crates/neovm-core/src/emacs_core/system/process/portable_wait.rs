//! Timer-serving process-output wait for hosts without process transports.

use crate::emacs_core::error::{EvalResult, LispCondition, expect_fixnum, expect_max_args, signal};
use crate::emacs_core::{Context, Value};

pub(crate) fn builtin_accept_process_output(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("accept-process-output", &args, 4)?;
    let process = args.first().copied().unwrap_or(Value::NIL);
    if !process.is_nil() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("processp"), process],
        ));
    }
    // GNU process.c: a nil PROCESS ignores JUST-THIS-ONE (including its
    // integer form that would otherwise suppress timers). The obsolete
    // MILLISEC convention accepts only fixnums, unlike fractional SECONDS.
    let seconds = args.get(1).copied().unwrap_or(Value::NIL);
    let milliseconds = args.get(2).copied().unwrap_or(Value::NIL);
    let duration = if !milliseconds.is_nil() {
        let millis = expect_fixnum(&milliseconds)? as f64;
        let secs = if seconds.is_nil() {
            0.0
        } else {
            expect_fixnum(&seconds)? as f64
        };
        secs + millis / 1000.0
    } else if seconds.is_nil() {
        0.0
    } else {
        seconds
            .as_fixnum()
            .map(|n| n as f64)
            .or_else(|| seconds.as_float())
            .ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("numberp"), seconds],
                )
            })?
    };
    // Reuse the editor's timer-aware, host-suspending duration wait. No OS
    // process is invented, and timer callbacks still run only on the VM thread.
    if duration > 0.0 {
        crate::emacs_core::timer::builtin_sleep_for(eval, vec![Value::make_float(duration)])?;
    } else {
        eval.service_timers_without_redisplay()?;
    }
    Ok(Value::NIL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_process_output_wait_accepts_the_no_process_case() {
        let mut eval = Context::new();
        for args in [vec![], vec![Value::NIL, Value::make_float(0.001)]] {
            assert_eq!(
                builtin_accept_process_output(&mut eval, args).unwrap(),
                Value::NIL
            );
        }
    }

    #[test]
    fn portable_process_output_wait_services_due_lisp_timers() {
        let mut eval = Context::new();
        eval.eval_str(
            r#"(progn
          (setq http-wait-timer-fired nil)
          (fset 'timer-event-handler
            (lambda (timer)
              (setq timer-list nil)
              (setq http-wait-timer-fired t)))
          (setq timer-list (list [nil 0 0 0 nil ignore nil nil nil nil])))"#,
        )
        .unwrap();
        builtin_accept_process_output(&mut eval, vec![Value::NIL, Value::make_float(0.001)])
            .unwrap();
        assert!(eval.eval_str("http-wait-timer-fired").unwrap().is_truthy());
    }

    #[test]
    fn portable_process_output_wait_preserves_gnu_argument_errors() {
        let mut eval = Context::new();
        for (args, predicate) in [
            (vec![Value::string("not a process")], "processp"),
            (vec![Value::NIL, Value::string("not seconds")], "numberp"),
            (
                vec![Value::NIL, Value::NIL, Value::make_float(1.0)],
                "fixnump",
            ),
            (
                vec![Value::NIL, Value::make_float(1.0), Value::fixnum(1)],
                "fixnump",
            ),
        ] {
            let error = builtin_accept_process_output(&mut eval, args).unwrap_err();
            let crate::emacs_core::error::Flow::Signal(signal) = error else {
                panic!("expected argument signal, got {error:?}");
            };
            assert_eq!(signal.symbol_name(), "wrong-type-argument");
            assert_eq!(signal.data[0], Value::symbol(predicate));
        }
        let error = builtin_accept_process_output(&mut eval, vec![Value::NIL; 5]).unwrap_err();
        let crate::emacs_core::error::Flow::Signal(signal) = error else {
            panic!("expected arity signal, got {error:?}");
        };
        assert_eq!(signal.symbol_name(), "wrong-number-of-arguments");
    }
}
