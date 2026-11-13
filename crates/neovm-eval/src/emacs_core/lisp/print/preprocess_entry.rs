//! GNU print.c sharing-preprocess entry, shared by every runtime.

use neovm_value::error::{EvalResult, expect_args};
use neovm_value::value::Value;

/// Keep these literal-name reads in this adapter. The ordinary shared name
/// reader became outlined after the adapter moved crates; this private bridge
/// retains the same intern-and-read operation with an explicit inline boundary.
#[inline(always)]
fn print_variable(eval: &crate::emacs_core::eval::Context, name: &str) -> Option<Value> {
    eval.builtin_var_value(neovm_value::intern::intern(name))
}

/// (print--preprocess OBJECT) -> nil
///
/// Extracts sharing info from OBJECT needed to print it: fills the
/// `print-number-table` hash when `print-circle' is non-nil, and does nothing
/// otherwise.  Mirrors GNU `Fprint_preprocess` (src/print.c).
#[doc(hidden)]
pub fn builtin_print_preprocess(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("print--preprocess", &args, 1)?;
    let object = args[0];

    // GNU: does nothing if `print-circle' is nil.
    let print_circle = print_variable(eval, "print-circle").is_some_and(|v| v.is_truthy());
    if !print_circle {
        return Ok(Value::NIL);
    }

    let print_gensym = print_variable(eval, "print-gensym").is_some_and(|v| v.is_truthy());
    let print_continuous_numbering =
        print_variable(eval, "print-continuous-numbering").is_some_and(|v| v.is_truthy());

    // GNU: `if (!HASH_TABLE_P (Vprint_number_table)) Vprint_number_table = make-hash-table :test eq`.
    let table_value = match print_variable(eval, "print-number-table") {
        Some(v) if v.is_hash_table() => v,
        _ => {
            let table = Value::hash_table(neovm_value::value::HashTableTest::Eq);
            eval.set_variable("print-number-table", table);
            table
        }
    };

    // Root the object and table across the (allocation-heavy) traversal.
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(object);
    eval.push_specpdl_root(table_value);
    super::preprocess_print_number_table(
        eval,
        &object,
        table_value,
        print_gensym,
        print_continuous_numbering,
    );
    eval.restore_specpdl_roots(roots);

    Ok(Value::NIL)
}
