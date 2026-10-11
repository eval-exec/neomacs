use super::*;

/// GNU `string_match_1` reads predeclared C variables such as
/// `Vcase_fold_search` and `Vinhibit_changing_match_data`; it does not resolve
/// their names for each match. Completion filtering calls `string-match`
/// repeatedly, so the steady-state path must likewise use canonical IDs.
#[test]
fn string_match_uses_predeclared_search_state_symbols() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let args = [Value::string("a"), Value::string("a")];

    eval.obarray_mut()
        .set_symbol_value("case-fold-search", Value::T);
    eval.obarray_mut()
        .set_symbol_value("inhibit-changing-match-data", Value::NIL);
    eval.obarray_mut()
        .set_symbol_value("char-script-table", Value::NIL);
    eval.obarray_mut()
        .set_symbol_value("word-combining-categories", Value::NIL);
    eval.obarray_mut()
        .set_symbol_value("word-separating-categories", Value::NIL);

    builtin_string_match_slice(&mut eval, &args).expect("warm string-match state");

    crate::emacs_core::intern::reset_intern_calls();
    let result = builtin_string_match_slice(&mut eval, &args).expect("string-match");

    assert_eq!(result, Value::fixnum(0));
    assert_eq!(
        crate::emacs_core::intern::intern_calls(),
        0,
        "steady-state string-match must use predeclared search-state symbols",
    );
}
