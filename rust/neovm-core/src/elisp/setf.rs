//! Generalized variables (setf) for Emacs Lisp.
//!
//! Implements `setf`, `push`, `pop`, `cl-incf`, `cl-decf`, and
//! basic `gv-define-setter` / `gv-define-simple-setter` support.

use super::error::{signal, EvalResult, Flow};
use super::expr::Expr;
use super::value::*;

// ---------------------------------------------------------------------------
// Argument / extraction helpers (local copies — this module is self-contained)
// ---------------------------------------------------------------------------

fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), other.clone()],
        )),
    }
}

fn expect_number(value: &Value) -> Result<f64, Flow> {
    match value {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        Value::Char(c) => Ok(*c as u32 as f64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("numberp"), other.clone()],
        )),
    }
}

fn has_float_pair(a: &Value, b: &Value) -> bool {
    matches!(a, Value::Float(_)) || matches!(b, Value::Float(_))
}

fn numeric_result(f: f64, was_float: bool) -> Value {
    if was_float {
        Value::Float(f)
    } else {
        Value::Int(f as i64)
    }
}

// ---------------------------------------------------------------------------
// Place reading: evaluate a place form and return its current value
// ---------------------------------------------------------------------------

/// Read the current value stored in PLACE.
fn read_place(eval: &mut super::eval::Evaluator, place: &Expr) -> EvalResult {
    eval.eval(place)
}

// ---------------------------------------------------------------------------
// Place writing: set a generalized place to a new value
// ---------------------------------------------------------------------------

/// Set a single generalized PLACE to VALUE.  Returns the value stored.
fn setf_place(
    eval: &mut super::eval::Evaluator,
    place: &Expr,
    value: Value,
) -> EvalResult {
    match place {
        // Simple variable
        Expr::Symbol(name) => {
            eval.assign(name, value.clone());
            Ok(value)
        }

        // Compound place: (accessor args...)
        Expr::List(items) if !items.is_empty() => {
            let Expr::Symbol(accessor) = &items[0] else {
                return Err(signal(
                    "invalid-generalized-variable",
                    vec![super::eval::quote_to_value(place)],
                ));
            };

            match accessor.as_str() {
                // (car FORM) -> (setcar FORM VALUE)
                "car" => {
                    if items.len() != 2 {
                        return Err(signal("wrong-number-of-arguments", vec![]));
                    }
                    let cell = eval.eval(&items[1])?;
                    match &cell {
                        Value::Cons(c) => {
                            c.lock().expect("poisoned").car = value.clone();
                            Ok(value)
                        }
                        _ => Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("consp"), cell],
                        )),
                    }
                }

                // (cdr FORM) -> (setcdr FORM VALUE)
                "cdr" => {
                    if items.len() != 2 {
                        return Err(signal("wrong-number-of-arguments", vec![]));
                    }
                    let cell = eval.eval(&items[1])?;
                    match &cell {
                        Value::Cons(c) => {
                            c.lock().expect("poisoned").cdr = value.clone();
                            Ok(value)
                        }
                        _ => Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("consp"), cell],
                        )),
                    }
                }

                // (aref ARRAY INDEX) -> (aset ARRAY INDEX VALUE)
                "aref" => {
                    if items.len() != 3 {
                        return Err(signal("wrong-number-of-arguments", vec![]));
                    }
                    let array = eval.eval(&items[1])?;
                    let idx_val = eval.eval(&items[2])?;
                    let idx = expect_int(&idx_val)? as usize;
                    match &array {
                        Value::Vector(v) => {
                            let mut elts = v.lock().expect("poisoned");
                            if idx >= elts.len() {
                                return Err(signal(
                                    "args-out-of-range",
                                    vec![array.clone(), idx_val],
                                ));
                            }
                            elts[idx] = value.clone();
                            Ok(value)
                        }
                        _ => Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("arrayp"), array],
                        )),
                    }
                }

                // (nth N LIST) -> set the car of the Nth cons cell
                "nth" => {
                    if items.len() != 3 {
                        return Err(signal("wrong-number-of-arguments", vec![]));
                    }
                    let n_val = eval.eval(&items[1])?;
                    let n = expect_int(&n_val)? as usize;
                    let list = eval.eval(&items[2])?;
                    let mut cursor = list;
                    for _ in 0..n {
                        match cursor {
                            Value::Cons(c) => {
                                cursor = c.lock().expect("poisoned").cdr.clone();
                            }
                            _ => {
                                return Err(signal(
                                    "args-out-of-range",
                                    vec![Value::Int(n as i64)],
                                ));
                            }
                        }
                    }
                    match &cursor {
                        Value::Cons(c) => {
                            c.lock().expect("poisoned").car = value.clone();
                            Ok(value)
                        }
                        _ => Err(signal(
                            "args-out-of-range",
                            vec![Value::Int(n as i64)],
                        )),
                    }
                }

                // (elt SEQ INDEX) -> set element of sequence
                "elt" => {
                    if items.len() != 3 {
                        return Err(signal("wrong-number-of-arguments", vec![]));
                    }
                    let seq = eval.eval(&items[1])?;
                    let idx_val = eval.eval(&items[2])?;
                    let idx = expect_int(&idx_val)? as usize;
                    match &seq {
                        Value::Vector(v) => {
                            let mut elts = v.lock().expect("poisoned");
                            if idx >= elts.len() {
                                return Err(signal(
                                    "args-out-of-range",
                                    vec![seq.clone(), idx_val],
                                ));
                            }
                            elts[idx] = value.clone();
                            Ok(value)
                        }
                        Value::Cons(_) | Value::Nil => {
                            let mut cursor = seq.clone();
                            for _ in 0..idx {
                                match cursor {
                                    Value::Cons(c) => {
                                        cursor =
                                            c.lock().expect("poisoned").cdr.clone();
                                    }
                                    _ => {
                                        return Err(signal(
                                            "args-out-of-range",
                                            vec![seq, idx_val],
                                        ));
                                    }
                                }
                            }
                            match &cursor {
                                Value::Cons(c) => {
                                    c.lock().expect("poisoned").car = value.clone();
                                    Ok(value)
                                }
                                _ => Err(signal(
                                    "args-out-of-range",
                                    vec![Value::Int(idx as i64)],
                                )),
                            }
                        }
                        other => Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("sequencep"), other.clone()],
                        )),
                    }
                }

                // (gethash KEY TABLE &optional DEFAULT) -> (puthash KEY VALUE TABLE)
                "gethash" => {
                    if items.len() < 3 || items.len() > 4 {
                        return Err(signal("wrong-number-of-arguments", vec![]));
                    }
                    let key = eval.eval(&items[1])?;
                    let table = eval.eval(&items[2])?;
                    match &table {
                        Value::HashTable(ht) => {
                            let mut ht = ht.lock().expect("poisoned");
                            let hk = key.to_hash_key(&ht.test);
                            ht.data.insert(hk, value.clone());
                            Ok(value)
                        }
                        _ => Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("hash-table-p"), table],
                        )),
                    }
                }

                // (symbol-value SYM) -> (set SYM VALUE)
                "symbol-value" => {
                    if items.len() != 2 {
                        return Err(signal("wrong-number-of-arguments", vec![]));
                    }
                    let sym = eval.eval(&items[1])?;
                    let name = sym.as_symbol_name().ok_or_else(|| {
                        signal(
                            "wrong-type-argument",
                            vec![Value::symbol("symbolp"), sym.clone()],
                        )
                    })?;
                    eval.assign(name, value.clone());
                    Ok(value)
                }

                // (symbol-function SYM) -> (fset SYM VALUE)
                "symbol-function" => {
                    if items.len() != 2 {
                        return Err(signal("wrong-number-of-arguments", vec![]));
                    }
                    let sym = eval.eval(&items[1])?;
                    let name = sym.as_symbol_name().ok_or_else(|| {
                        signal(
                            "wrong-type-argument",
                            vec![Value::symbol("symbolp"), sym.clone()],
                        )
                    })?;
                    eval.obarray_mut().set_symbol_function(name, value.clone());
                    Ok(value)
                }

                // (buffer-name) -> (rename-buffer VALUE)
                "buffer-name" => {
                    // The new value must be a string.
                    let new_name = match &value {
                        Value::Str(s) => (**s).clone(),
                        _ => {
                            return Err(signal(
                                "wrong-type-argument",
                                vec![Value::symbol("stringp"), value],
                            ))
                        }
                    };
                    let buf = eval.buffers.current_buffer_mut().ok_or_else(|| {
                        signal("error", vec![Value::string("No current buffer")])
                    })?;
                    buf.name = new_name;
                    Ok(value)
                }

                // (point) -> (goto-char VALUE)
                "point" => {
                    let pos = expect_int(&value)?;
                    let buf =
                        eval.buffers.current_buffer_mut().ok_or_else(|| {
                            signal(
                                "error",
                                vec![Value::string("No current buffer")],
                            )
                        })?;
                    let char_pos =
                        if pos > 0 { pos as usize - 1 } else { 0 };
                    let byte_pos = buf
                        .text
                        .char_to_byte(char_pos.min(buf.text.char_count()));
                    buf.goto_char(byte_pos);
                    Ok(value)
                }

                // (mark) -> (set-mark VALUE)
                "mark" => {
                    let pos = expect_int(&value)? as usize;
                    let buf =
                        eval.buffers.current_buffer_mut().ok_or_else(|| {
                            signal(
                                "error",
                                vec![Value::string("No current buffer")],
                            )
                        })?;
                    let char_pos = if pos > 0 { pos - 1 } else { 0 };
                    let byte_pos = buf
                        .text
                        .char_to_byte(char_pos.min(buf.text.char_count()));
                    buf.set_mark(byte_pos);
                    Ok(value)
                }

                // (default-value SYM) -> (set-default SYM VALUE)
                "default-value" => {
                    if items.len() != 2 {
                        return Err(signal("wrong-number-of-arguments", vec![]));
                    }
                    let sym = eval.eval(&items[1])?;
                    let name = sym.as_symbol_name().ok_or_else(|| {
                        signal(
                            "wrong-type-argument",
                            vec![Value::symbol("symbolp"), sym.clone()],
                        )
                    })?;
                    eval.obarray_mut()
                        .set_symbol_value(name, value.clone());
                    Ok(value)
                }

                // (overlay-get OVERLAY PROP) -> (overlay-put OVERLAY PROP VALUE)
                "overlay-get" => {
                    if items.len() != 3 {
                        return Err(signal("wrong-number-of-arguments", vec![]));
                    }
                    let overlay = eval.eval(&items[1])?;
                    let prop = eval.eval(&items[2])?;
                    // Delegate to overlay-put through eval.apply
                    eval.apply(
                        Value::Symbol("overlay-put".to_string()),
                        vec![overlay, prop, value.clone()],
                    )?;
                    Ok(value)
                }

                // Unknown accessor — check for a user-defined gv-setter in the obarray
                other => {
                    // Look for a gv-setter property on the symbol
                    if let Some(setter_name) = eval
                        .obarray()
                        .get_property(other, "gv-setter")
                        .cloned()
                    {
                        match &setter_name {
                            // Simple setter: property value is a symbol name string
                            Value::Symbol(setter) => {
                                let mut setter_args = Vec::new();
                                // Evaluate the sub-expressions of the place
                                for sub_expr in &items[1..] {
                                    setter_args.push(eval.eval(sub_expr)?);
                                }
                                setter_args.push(value.clone());
                                eval.apply(
                                    Value::Symbol(setter.clone()),
                                    setter_args,
                                )?;
                                Ok(value)
                            }
                            // Lambda setter: call the lambda with (VALUE args...)
                            Value::Lambda(_) => {
                                let mut setter_args = vec![value.clone()];
                                for sub_expr in &items[1..] {
                                    setter_args.push(eval.eval(sub_expr)?);
                                }
                                eval.apply(setter_name, setter_args)?;
                                Ok(value)
                            }
                            _ => Err(signal(
                                "invalid-generalized-variable",
                                vec![super::eval::quote_to_value(place)],
                            )),
                        }
                    } else {
                        Err(signal(
                            "invalid-generalized-variable",
                            vec![super::eval::quote_to_value(place)],
                        ))
                    }
                }
            }
        }

        _ => Err(signal(
            "invalid-generalized-variable",
            vec![super::eval::quote_to_value(place)],
        )),
    }
}

// ===========================================================================
// setf special form
// ===========================================================================

/// `(setf PLACE VALUE [PLACE VALUE] ...)` -- set generalized places.
pub(crate) fn sf_setf(
    eval: &mut super::eval::Evaluator,
    tail: &[Expr],
) -> EvalResult {
    if tail.is_empty() {
        return Ok(Value::Nil);
    }
    if tail.len() % 2 != 0 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }

    let mut last = Value::Nil;
    let mut i = 0;
    while i < tail.len() {
        let place = &tail[i];
        let value = eval.eval(&tail[i + 1])?;
        last = setf_place(eval, place, value)?;
        i += 2;
    }
    Ok(last)
}

// ===========================================================================
// push / pop
// ===========================================================================

/// `(push ELEMENT PLACE)` -- push ELEMENT onto the list stored in PLACE.
/// Returns the new list.
pub(crate) fn sf_push(
    eval: &mut super::eval::Evaluator,
    tail: &[Expr],
) -> EvalResult {
    if tail.len() != 2 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }
    let element = eval.eval(&tail[0])?;
    let old_list = read_place(eval, &tail[1])?;
    let new_list = Value::cons(element, old_list);
    setf_place(eval, &tail[1], new_list.clone())?;
    Ok(new_list)
}

/// `(pop PLACE)` -- remove and return the first element of the list in PLACE.
pub(crate) fn sf_pop(
    eval: &mut super::eval::Evaluator,
    tail: &[Expr],
) -> EvalResult {
    if tail.len() != 1 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }
    let list = read_place(eval, &tail[0])?;
    match &list {
        Value::Nil => {
            // Already empty — nothing to pop, place stays nil
            Ok(Value::Nil)
        }
        Value::Cons(c) => {
            let pair = c.lock().expect("poisoned");
            let first = pair.car.clone();
            let rest = pair.cdr.clone();
            drop(pair); // release lock before writing back
            setf_place(eval, &tail[0], rest)?;
            Ok(first)
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("listp"), other.clone()],
        )),
    }
}

// ===========================================================================
// cl-incf / cl-decf
// ===========================================================================

/// `(cl-incf PLACE &optional DELTA)` -- increment PLACE by DELTA (default 1).
pub(crate) fn sf_cl_incf(
    eval: &mut super::eval::Evaluator,
    tail: &[Expr],
) -> EvalResult {
    if tail.is_empty() || tail.len() > 2 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }
    let current = read_place(eval, &tail[0])?;
    let delta = if tail.len() > 1 {
        eval.eval(&tail[1])?
    } else {
        Value::Int(1)
    };

    let float_mode = has_float_pair(&current, &delta);
    let cur_f = expect_number(&current)?;
    let del_f = expect_number(&delta)?;
    let new_val = numeric_result(cur_f + del_f, float_mode);
    setf_place(eval, &tail[0], new_val.clone())?;
    Ok(new_val)
}

/// `(cl-decf PLACE &optional DELTA)` -- decrement PLACE by DELTA (default 1).
pub(crate) fn sf_cl_decf(
    eval: &mut super::eval::Evaluator,
    tail: &[Expr],
) -> EvalResult {
    if tail.is_empty() || tail.len() > 2 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }
    let current = read_place(eval, &tail[0])?;
    let delta = if tail.len() > 1 {
        eval.eval(&tail[1])?
    } else {
        Value::Int(1)
    };

    let float_mode = has_float_pair(&current, &delta);
    let cur_f = expect_number(&current)?;
    let del_f = expect_number(&delta)?;
    let new_val = numeric_result(cur_f - del_f, float_mode);
    setf_place(eval, &tail[0], new_val.clone())?;
    Ok(new_val)
}

// ===========================================================================
// gv-define-setter / gv-define-simple-setter
// ===========================================================================

/// `(gv-define-simple-setter GETTER SETTER)` -- register SETTER as the
/// setf-expander for GETTER.  The setter is called as (SETTER args... VALUE).
///
/// Stores the setter symbol name as the `gv-setter` property on the getter's
/// symbol in the obarray.
pub(crate) fn sf_gv_define_simple_setter(
    eval: &mut super::eval::Evaluator,
    tail: &[Expr],
) -> EvalResult {
    if tail.len() < 2 || tail.len() > 3 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }
    let getter_name = match &tail[0] {
        Expr::Symbol(s) => s.as_str(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), super::eval::quote_to_value(other)],
            ))
        }
    };
    let setter_sym = match &tail[1] {
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), super::eval::quote_to_value(other)],
            ))
        }
    };
    eval.obarray_mut()
        .put_property(getter_name, "gv-setter", setter_sym);
    Ok(Value::Symbol(getter_name.to_string()))
}

/// `(gv-define-setter GETTER LAMBDA)` -- register a lambda as the
/// setf-expander for GETTER.  The lambda is called as (LAMBDA VALUE args...).
///
/// Stores the lambda as the `gv-setter` property on the getter's symbol.
pub(crate) fn sf_gv_define_setter(
    eval: &mut super::eval::Evaluator,
    tail: &[Expr],
) -> EvalResult {
    // (gv-define-setter NAME (VALUE-VAR ARGS...) BODY...)
    // We compile the arglist + body into a lambda and store it.
    if tail.len() < 3 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }
    let getter_name = match &tail[0] {
        Expr::Symbol(s) => s.as_str(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), super::eval::quote_to_value(other)],
            ))
        }
    };
    // Build a lambda from the arglist and body
    let lambda = eval.eval_lambda(&tail[1..])?;
    eval.obarray_mut()
        .put_property(getter_name, "gv-setter", lambda);
    Ok(Value::Symbol(getter_name.to_string()))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::{format_eval_result, parse_forms};
    use super::super::eval::Evaluator;

    fn eval_one(src: &str) -> String {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        let result = ev.eval_expr(&forms[0]);
        format_eval_result(&result)
    }

    fn eval_all(src: &str) -> Vec<String> {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        ev.eval_forms(&forms)
            .iter()
            .map(format_eval_result)
            .collect()
    }

    fn eval_last(src: &str) -> String {
        let results = eval_all(src);
        results.last().unwrap().clone()
    }

    // -- helper to register setf/push/pop/cl-incf/cl-decf as special forms --
    // Since we are not wired into eval.rs yet, we test through a mini evaluator
    // that manually calls our functions.  However, the test helpers use full
    // eval, so we need the special forms to be known.  We work around this by
    // constructing the Expr tree manually and invoking the sf_* functions
    // directly.

    fn make_ev() -> Evaluator {
        Evaluator::new()
    }

    // -----------------------------------------------------------------------
    // setf — simple variables
    // -----------------------------------------------------------------------

    #[test]
    fn setf_simple_variable() {
        let mut ev = make_ev();
        // (setq x 10)
        let forms = parse_forms("(setq x 10)").unwrap();
        ev.eval_expr(&forms[0]).unwrap();

        // (setf x 20)
        let forms = parse_forms("(setf x 20)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_setf(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "20");
        // Verify x is now 20
        let forms = parse_forms("x").unwrap();
        let result = ev.eval_expr(&forms[0]).unwrap();
        assert_eq!(format!("{}", result), "20");
    }

    #[test]
    fn setf_multiple_pairs() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq a 1) (setq b 2)").unwrap();
        for f in &setup {
            ev.eval_expr(f).unwrap();
        }

        let forms = parse_forms("(setf a 10 b 20)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_setf(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "20"); // last value

        let forms = parse_forms("a").unwrap();
        assert_eq!(format!("{}", ev.eval_expr(&forms[0]).unwrap()), "10");
        let forms = parse_forms("b").unwrap();
        assert_eq!(format!("{}", ev.eval_expr(&forms[0]).unwrap()), "20");
    }

    // -----------------------------------------------------------------------
    // setf — car / cdr
    // -----------------------------------------------------------------------

    #[test]
    fn setf_car() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq lst (list 1 2 3))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(setf (car lst) 99)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("lst").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "(99 2 3)");
    }

    #[test]
    fn setf_cdr() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq lst (list 1 2 3))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(setf (cdr lst) '(20 30))").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("lst").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "(1 20 30)");
    }

    // -----------------------------------------------------------------------
    // setf — aref
    // -----------------------------------------------------------------------

    #[test]
    fn setf_aref() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq v (vector 10 20 30))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(setf (aref v 1) 99)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("(aref v 1)").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "99");
    }

    #[test]
    fn setf_aref_out_of_range() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq v (vector 10 20 30))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(setf (aref v 5) 99)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_setf(&mut ev, &items[1..]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // setf — nth
    // -----------------------------------------------------------------------

    #[test]
    fn setf_nth() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq lst (list 10 20 30 40))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(setf (nth 2 lst) 99)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("lst").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "(10 20 99 40)");
    }

    // -----------------------------------------------------------------------
    // setf — elt (list and vector)
    // -----------------------------------------------------------------------

    #[test]
    fn setf_elt_list() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq lst (list 'a 'b 'c))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(setf (elt lst 1) 'z)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("lst").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "(a z c)");
    }

    #[test]
    fn setf_elt_vector() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq v (vector 'a 'b 'c))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(setf (elt v 2) 'z)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("(aref v 2)").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "z");
    }

    // -----------------------------------------------------------------------
    // setf — gethash
    // -----------------------------------------------------------------------

    #[test]
    fn setf_gethash() {
        let mut ev = make_ev();
        let setup =
            parse_forms("(setq ht (make-hash-table :test 'equal))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms(r#"(setf (gethash "key" ht) 42)"#).unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms(r#"(gethash "key" ht)"#).unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "42");
    }

    // -----------------------------------------------------------------------
    // setf — symbol-value
    // -----------------------------------------------------------------------

    #[test]
    fn setf_symbol_value() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq myvar 10)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms =
            parse_forms("(setf (symbol-value 'myvar) 42)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("myvar").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "42");
    }

    // -----------------------------------------------------------------------
    // setf — symbol-function
    // -----------------------------------------------------------------------

    #[test]
    fn setf_symbol_function() {
        let mut ev = make_ev();
        let setup = parse_forms("(defun dummy () 1)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms(
            "(setf (symbol-function 'dummy) (lambda () 99))",
        )
        .unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("(dummy)").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "99");
    }

    // -----------------------------------------------------------------------
    // setf — buffer-name
    // -----------------------------------------------------------------------

    #[test]
    fn setf_buffer_name() {
        let mut ev = make_ev();
        let setup = parse_forms(
            r#"(get-buffer-create "old-name") (set-buffer "old-name")"#,
        )
        .unwrap();
        for f in &setup {
            ev.eval_expr(f).unwrap();
        }

        let forms =
            parse_forms(r#"(setf (buffer-name) "new-name")"#).unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("(buffer-name)").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "\"new-name\"");
    }

    // -----------------------------------------------------------------------
    // setf — point
    // -----------------------------------------------------------------------

    #[test]
    fn setf_point() {
        let mut ev = make_ev();
        let setup = parse_forms(
            r#"(get-buffer-create "pt") (set-buffer "pt") (insert "hello")"#,
        )
        .unwrap();
        for f in &setup {
            ev.eval_expr(f).unwrap();
        }

        let forms = parse_forms("(setf (point) 3)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("(point)").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "3");
    }

    // -----------------------------------------------------------------------
    // setf — mark
    // -----------------------------------------------------------------------

    #[test]
    fn setf_mark() {
        let mut ev = make_ev();
        let setup = parse_forms(
            r#"(get-buffer-create "mk") (set-buffer "mk") (insert "hello") (set-mark 1)"#,
        )
        .unwrap();
        for f in &setup {
            ev.eval_expr(f).unwrap();
        }

        let forms = parse_forms("(setf (mark) 4)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("(mark)").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "4");
    }

    // -----------------------------------------------------------------------
    // setf — default-value
    // -----------------------------------------------------------------------

    #[test]
    fn setf_default_value() {
        let mut ev = make_ev();
        let setup = parse_forms("(defvar myglob 10)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms =
            parse_forms("(setf (default-value 'myglob) 99)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("myglob").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "99");
    }

    // -----------------------------------------------------------------------
    // setf — odd arg count error
    // -----------------------------------------------------------------------

    #[test]
    fn setf_odd_args_error() {
        let mut ev = make_ev();
        let forms = parse_forms("(setf x)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_setf(&mut ev, &items[1..]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // setf — invalid place
    // -----------------------------------------------------------------------

    #[test]
    fn setf_invalid_place() {
        let mut ev = make_ev();
        let forms = parse_forms("(setf 42 10)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_setf(&mut ev, &items[1..]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // push / pop
    // -----------------------------------------------------------------------

    #[test]
    fn push_onto_variable() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq lst '(2 3))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(push 1 lst)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_push(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "(1 2 3)");

        let check = parse_forms("lst").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "(1 2 3)");
    }

    #[test]
    fn push_onto_nil() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq lst nil)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(push 'a lst)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_push(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("lst").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "(a)");
    }

    #[test]
    fn pop_from_list() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq lst '(1 2 3))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(pop lst)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_pop(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "1");

        let check = parse_forms("lst").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "(2 3)");
    }

    #[test]
    fn pop_from_empty() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq lst nil)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(pop lst)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_pop(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "nil");
    }

    #[test]
    fn push_pop_roundtrip() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq stk nil)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        // Push three elements
        for val in &["'a", "'b", "'c"] {
            let src = format!("(push {} stk)", val);
            let forms = parse_forms(&src).unwrap();
            let Expr::List(items) = &forms[0] else { panic!() };
            sf_push(&mut ev, &items[1..]).unwrap();
        }

        // Stack should be (c b a)
        let check = parse_forms("stk").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "(c b a)");

        // Pop all three
        let mut popped = Vec::new();
        for _ in 0..3 {
            let forms = parse_forms("(pop stk)").unwrap();
            let Expr::List(items) = &forms[0] else { panic!() };
            let v = sf_pop(&mut ev, &items[1..]).unwrap();
            popped.push(format!("{}", v));
        }
        assert_eq!(popped, vec!["c", "b", "a"]);

        // Stack should be nil
        let check = parse_forms("stk").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "nil");
    }

    // -----------------------------------------------------------------------
    // cl-incf / cl-decf
    // -----------------------------------------------------------------------

    #[test]
    fn cl_incf_default_delta() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq x 10)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(cl-incf x)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_cl_incf(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "11");

        let check = parse_forms("x").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "11");
    }

    #[test]
    fn cl_incf_with_delta() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq x 10)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(cl-incf x 5)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_cl_incf(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "15");
    }

    #[test]
    fn cl_incf_float() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq x 1.5)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(cl-incf x 0.5)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_cl_incf(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "2.0");
    }

    #[test]
    fn cl_decf_default_delta() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq x 10)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(cl-decf x)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_cl_decf(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "9");
    }

    #[test]
    fn cl_decf_with_delta() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq x 10)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(cl-decf x 3)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_cl_decf(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "7");
    }

    #[test]
    fn cl_incf_compound_place() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq lst (list 10 20 30))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        // (cl-incf (nth 1 lst) 5) should set element 1 to 25
        let forms = parse_forms("(cl-incf (nth 1 lst) 5)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_cl_incf(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "25");

        let check = parse_forms("lst").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "(10 25 30)");
    }

    #[test]
    fn cl_decf_compound_place() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq v (vector 100 200 300))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(cl-decf (aref v 0) 50)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_cl_decf(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "50");

        let check = parse_forms("(aref v 0)").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "50");
    }

    // -----------------------------------------------------------------------
    // gv-define-simple-setter
    // -----------------------------------------------------------------------

    #[test]
    fn gv_define_simple_setter_basic() {
        let mut ev = make_ev();
        // Define a simple getter/setter pair using obarray functions
        let setup = parse_forms(
            "(defun my-get (obj) (car obj))
             (defun my-set (obj val) (setcar obj val))
             (setq pair (cons 1 2))",
        )
        .unwrap();
        for f in &setup {
            ev.eval_expr(f).unwrap();
        }

        // Register the setter
        let forms =
            parse_forms("(gv-define-simple-setter my-get my-set)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_gv_define_simple_setter(&mut ev, &items[1..]).unwrap();

        // Now use setf with the custom getter
        let forms = parse_forms("(setf (my-get pair) 99)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        // my-set is called as (my-set pair 99), setting car to 99
        let check = parse_forms("(car pair)").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "99");
    }

    // -----------------------------------------------------------------------
    // setf — empty returns nil
    // -----------------------------------------------------------------------

    #[test]
    fn setf_empty_returns_nil() {
        let mut ev = make_ev();
        let forms = parse_forms("(setf)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_setf(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "nil");
    }

    // -----------------------------------------------------------------------
    // setf — unknown compound place signals error
    // -----------------------------------------------------------------------

    #[test]
    fn setf_unknown_accessor_signals_error() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq x 1)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(setf (totally-unknown x) 42)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_setf(&mut ev, &items[1..]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // push with compound place
    // -----------------------------------------------------------------------

    #[test]
    fn push_onto_car_place() {
        let mut ev = make_ev();
        // Create a cons whose car is a list
        let setup = parse_forms("(setq cell (cons '(2 3) nil))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(push 1 (car cell))").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_push(&mut ev, &items[1..]).unwrap();

        let check = parse_forms("(car cell)").unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "(1 2 3)");
    }

    // -----------------------------------------------------------------------
    // cl-incf / cl-decf wrong-type errors
    // -----------------------------------------------------------------------

    #[test]
    fn cl_incf_non_number_signals_error() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq x 'hello)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(cl-incf x)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_cl_incf(&mut ev, &items[1..]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Arity errors for push/pop/incf/decf
    // -----------------------------------------------------------------------

    #[test]
    fn push_wrong_arity() {
        let mut ev = make_ev();
        let forms = parse_forms("(push 1)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_push(&mut ev, &items[1..]);
        assert!(result.is_err());
    }

    #[test]
    fn pop_wrong_arity() {
        let mut ev = make_ev();
        let forms = parse_forms("(pop)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_pop(&mut ev, &items[1..]);
        assert!(result.is_err());
    }

    #[test]
    fn cl_incf_wrong_arity() {
        let mut ev = make_ev();
        let forms = parse_forms("(cl-incf)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_cl_incf(&mut ev, &items[1..]);
        assert!(result.is_err());
    }

    #[test]
    fn cl_decf_wrong_arity() {
        let mut ev = make_ev();
        let forms = parse_forms("(cl-decf x 1 2)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_cl_decf(&mut ev, &items[1..]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // setf — gethash with default arg
    // -----------------------------------------------------------------------

    #[test]
    fn setf_gethash_with_default() {
        let mut ev = make_ev();
        let setup =
            parse_forms("(setq ht (make-hash-table :test 'equal))").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        // (setf (gethash "key" ht 'default) 42)
        let forms =
            parse_forms(r#"(setf (gethash "key" ht 'default) 42)"#).unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        sf_setf(&mut ev, &items[1..]).unwrap();

        let check = parse_forms(r#"(gethash "key" ht)"#).unwrap();
        let result = ev.eval_expr(&check[0]).unwrap();
        assert_eq!(format!("{}", result), "42");
    }

    // -----------------------------------------------------------------------
    // Mixed int/float for incf/decf
    // -----------------------------------------------------------------------

    #[test]
    fn cl_incf_int_plus_float_gives_float() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq x 10)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(cl-incf x 0.5)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_cl_incf(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "10.5");
    }

    #[test]
    fn cl_decf_float_minus_int_gives_float() {
        let mut ev = make_ev();
        let setup = parse_forms("(setq x 5.0)").unwrap();
        ev.eval_expr(&setup[0]).unwrap();

        let forms = parse_forms("(cl-decf x 2)").unwrap();
        let Expr::List(items) = &forms[0] else { panic!() };
        let result = sf_cl_decf(&mut ev, &items[1..]).unwrap();
        assert_eq!(format!("{}", result), "3.0");
    }
}
