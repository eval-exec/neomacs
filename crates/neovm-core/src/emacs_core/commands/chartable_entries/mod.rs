//! Char-table Lisp entries that stay in the core command layer.
//!
//! `unicode-property-table-internal` can lazily load `international/<file>`
//! through the evaluator's `load` entry, so under effect-review R0 the Lisp
//! entry stays in core; the property scan and the ASCII cache helpers are
//! called downward into the value/text layers.

use crate::emacs_core::error::{EvalResult, Flow, expect_args};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

/// GNU `char-code-property-alist` cell for PROP: the alist entry whose car is
/// `eq` to PROP, read from the evaluator's visible variable value.
fn char_code_property_cell(eval: &Context, prop: Value) -> Result<Value, Flow> {
    let alist = eval
        .builtin_var_value_by_name("char-code-property-alist")
        .unwrap_or(Value::NIL);
    neovm_value::chartab::assq_cell_eq(prop, alist, eval.symbol_mode())
}

/// `(unicode-property-table-internal PROP)`.
///
/// GNU's `chartab.c:uniprop_table` lazily loads `international/<file>` when
/// `char-code-property-alist` stores a string, then the public primitive returns
/// the alist cdr even for property tables whose decoder is Lisp rather than the
/// C fast-path decoder.  That distinction matters for `name`/`old-name`, whose
/// generated tables use byte-code decoder functions in extra slots.
pub(crate) fn builtin_unicode_property_table_internal(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("unicode-property-table-internal", &args, 1)?;
    let prop = args[0];
    let mut cell = char_code_property_cell(eval, prop)?;
    if cell.is_nil() {
        return Ok(Value::NIL);
    }

    let table = cell.cons_cdr();
    if table.is_string() {
        let Some(file_name) = table
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        else {
            return Ok(table);
        };
        let load_name = Value::string(format!("international/{file_name}"));
        let _ = crate::emacs_core::load::builtin_load_in_vm_runtime(
            eval,
            &[load_name, Value::T, Value::T, Value::T, Value::T],
        )?;
        cell = char_code_property_cell(eval, prop)?;
        if cell.is_nil() {
            return Ok(Value::NIL);
        }
    }

    let table = cell.cons_cdr();
    neovm_value::chartab::prepare_uniprop_ascii_cache(&table, eval.symbol_mode());
    Ok(table)
}
