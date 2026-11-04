//! Evaluator-side entry point for `signal`.
//!
//! `signal` is the one caller-visible signal path that needs the evaluator:
//! it reads symbol identity from the obarray and dispatches the resulting
//! flow on the evaluator's handler stack.  The flow constructors and the
//! `signal` primitive stay in the error layer; these wrappers sit above it.

#[doc(hidden)]
pub use neovm_value::error::{
    EvalResult, Flow, FlowKind, LispCondition, signal, signal_suppressed,
};
use neovm_value::errors::{
    build_signal_flow, build_signal_flow_id, build_signal_flow_id_suppressed,
};
use neovm_value::intern::intern;
use neovm_value::value::Value;

use super::Context;

fn build_peculiar_signal_flow(eval: &Context, error_object: Value) -> Flow {
    let symbol_mode = eval.symbol_mode();
    if !error_object.is_cons() {
        unreachable!("peculiar signal error object must be a cons");
    };
    let pair_car = error_object.cons_car();
    let pair_cdr = error_object.cons_cdr();
    let error_symbol = pair_car;
    let data = pair_cdr;

    let Some(symbol_id) = error_symbol.as_symbol_id(symbol_mode) else {
        return signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), error_symbol],
        );
    };
    // Read `error-conditions' by identity so an uninterned error symbol is
    // honoured (see `builtin_signal`).
    if symbol_id != intern("error")
        && symbol_id != intern("quit")
        && eval
            .obarray
            .get_property_id(symbol_id, intern("error-conditions"))
            .is_none()
    {
        return signal_suppressed("error", vec![Value::string("Invalid error symbol")]);
    }

    build_signal_flow_id_suppressed(symbol_id, data)
}

/// `(signal CONS)` with nil DATA: the cons *is* the whole error object,
/// re-raised as-is.  GNU `signal_or_quit` sets `real_error_symbol = XCAR(error)`
/// and keeps `XCDR(error)` as the data.  Unlike `build_peculiar_signal_flow`
/// (which models OOM and suppresses the signal hook), this is a normal
/// `Fsignal` call, so the signal hook runs.
fn build_resignal_flow(eval: &Context, error_object: Value) -> Flow {
    let symbol_mode = eval.symbol_mode();
    let error_symbol = error_object.cons_car();
    let data = error_object.cons_cdr();

    // `Fget (real_error_symbol, Qerror_conditions)` runs `CHECK_SYMBOL`, so a
    // non-symbol car signals `wrong-type-argument symbolp <car>`.
    let Some(symbol_id) = error_symbol.as_symbol_id(symbol_mode) else {
        return signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), error_symbol],
        );
    };
    // Read `error-conditions' by identity so an uninterned error symbol is
    // honoured (see `builtin_signal`).
    if symbol_id != intern("error")
        && symbol_id != intern("quit")
        && eval
            .obarray
            .get_property_id(symbol_id, intern("error-conditions"))
            .is_none()
    {
        return signal(
            "error",
            vec![
                Value::string("Invalid error symbol"),
                Value::from_sym_id(symbol_id),
            ],
        );
    }

    build_signal_flow_id(symbol_id, data)
}

/// Eval-aware `signal`, including GNU's "peculiar error" handling for
/// `nil` as the public error symbol.
#[doc(hidden)]
pub fn builtin_signal(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    if args.is_empty() || args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("signal"), Value::fixnum(args.len() as i64)],
        ));
    }
    // GNU 31.0.90: `signal` is DEFUN(1, 2) — DATA is optional, defaulting to nil.
    let data = args.get(1).copied().unwrap_or(Value::NIL);

    if args[0].is_nil() {
        let flow = if data.is_cons() {
            build_peculiar_signal_flow(eval, data)
        } else {
            build_signal_flow("error", data)
        };
        return dispatch_signal_flow(eval, flow);
    }

    // GNU `signal_or_quit` (src/eval.c:1930):
    //   error = (!SYMBOLP (error_symbol) && NILP (data)) ? error_symbol
    //                                                     : Fcons (error_symbol, data);
    //   real_error_symbol = CONSP (error) ? XCAR (error) : error_symbol;
    // So when the public error symbol is itself a cons and DATA is nil, the
    // cons *is* the whole error object, re-raised as-is: its car becomes the
    // real error symbol and its cdr the data.  This is the re-signal idiom
    // `(signal (list 'scan-error "msg" 2 4))` used by ~91 lisp/ call sites.
    if args[0].is_cons() && data.is_nil() {
        let flow = build_resignal_flow(eval, args[0]);
        return dispatch_signal_flow(eval, flow);
    }

    // Preserve the *identity* of the error symbol (GNU `Fsignal` passes the
    // actual symbol object to `signal_or_quit`).  An uninterned symbol given
    // `error-conditions' by `define-error' must keep its SymId so condition
    // matching and `canonicalize_signal_symbol' read the right plist; building
    // the flow from the symbol's name would re-intern to a different symbol.
    let Some(symbol_id) = args[0].as_symbol_id(symbol_mode) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    };

    let flow = build_signal_flow_id(symbol_id, data);

    dispatch_signal_flow(eval, flow)
}

fn dispatch_signal_flow(eval: &mut Context, flow: Flow) -> EvalResult {
    match flow.into_kind() {
        FlowKind::Signal(sig) => match eval.dispatch_signal_if_needed(sig) {
            Ok(dispatched) => Err(Flow::signal_boxed(dispatched)),
            Err(flow) => Err(flow),
        },
        other => Err(Flow::from_kind(other)),
    }
}
