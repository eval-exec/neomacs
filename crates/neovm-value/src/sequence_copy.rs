//! Sequence-copy core (GNU `fns.c` `Fcopy_sequence`).
//!
//! `copy_sequence_with_symbols` is the whole sequence dispatch — list,
//! string, vector, char-table, record, bool-vector — moved below the
//! evaluator with the stage-3 split (C12) once the char-table copy landed
//! in `chartab` (C7a). The subr wrappers and the Context-routed entry
//! points stay in `builtins/cons_list.rs`, resolving the owning symbol
//! policy into a `SymbolMode` before the call.
//!
//! The list arm is the observed/unobserved scan pair over `list_scan`:
//! this source walk runs no Lisp callback or collecting safe point, so
//! each mutator selects its own capture policy once.

use crate::boolvec::copy_bool_vector;
use crate::chartab::copy_char_table_with_symbols;
use crate::error::{EvalResult, LispCondition, signal};
use crate::list_scan::{for_each_tail_cycle_tail, scan_car, scan_cdr, unobserved_list_scan};
use crate::tagged::SymbolMode;
use crate::value::{
    Value, ValueKind, VecLikeType, get_string_text_properties_table_for_value,
    set_string_text_properties_table_for_value,
};

/// One sequence dispatch; only char-table copying resolves symbol policy.
#[doc(hidden)]
pub fn copy_sequence_with_symbols(arg: Value, symbols: SymbolMode) -> EvalResult {
    match arg.kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => {
            if unobserved_list_scan() {
                copy_list_sequence_scan::<false>(arg)
            } else {
                copy_list_sequence_scan::<true>(arg)
            }
        }
        ValueKind::String => {
            let string = arg
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload");
            // GNU Emacs: (copy-sequence "") returns "" itself (eq).
            if string.is_empty() {
                return Ok(arg);
            }
            let new_val = Value::heap_string(string.clone());
            // Copy text properties
            if new_val.is_string()
                && let Some(table) = get_string_text_properties_table_for_value(arg)
            {
                set_string_text_properties_table_for_value(
                    new_val,
                    table.copy_interval_plist_spines(),
                );
            }
            Ok(new_val)
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let elems = arg.as_vector_data().unwrap().clone();
            // GNU Emacs: (copy-sequence (vector)) returns the same empty vector (eq).
            if elems.is_empty() {
                return Ok(arg);
            }
            Ok(Value::vector(elems))
        }
        ValueKind::Veclike(VecLikeType::CharTable) => copy_char_table_with_symbols(arg, symbols)
            .ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("sequencep"), arg],
                )
            }),
        ValueKind::Veclike(VecLikeType::Record) => {
            let items = arg.as_record_data().unwrap().clone();
            Ok(Value::make_record(items))
        }
        // GNU `Fcopy_sequence`: always a fresh bool-vector.
        ValueKind::Veclike(VecLikeType::BoolVector) => {
            Ok(copy_bool_vector(&arg).expect("a bool-vector"))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), arg],
        )),
    }
}

/// This source walk runs no Lisp callback or collecting safe point. Each
/// mutator selects its own capture policy once; fresh-result setters keep
/// their normal journals and barriers in both traversal variants.
#[inline]
fn copy_list_sequence_scan<const OBSERVED: bool>(arg: Value) -> EvalResult {
    let copy = Value::cons(scan_car::<OBSERVED>(arg), Value::NIL);
    let mut prev = copy;
    let mut tail = scan_cdr::<OBSERVED>(arg);
    let mut tortoise = tail;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2u16;

    while tail.is_cons() {
        let next = Value::cons(scan_car::<OBSERVED>(tail), Value::NIL);
        prev.set_cdr(next);
        prev = next;

        tail = scan_cdr::<OBSERVED>(tail);
        if tail.is_cons()
            && let Some(cycle_tail) =
                for_each_tail_cycle_tail(tail, &mut tortoise, &mut max, &mut n, &mut q)
        {
            return Err(signal(LispCondition::CircularList, vec![cycle_tail]));
        }
    }

    if tail.is_nil() {
        Ok(copy)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), tail],
        ))
    }
}
