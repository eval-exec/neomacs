//! Positioned charset arguments reach policy-aware registered builtins.
use crate::common::{
    assert_ok_eq, eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set,
};

fn pair_form(call: &str, name: &str, positioned: bool, nonsymbol: bool) -> String {
    let argument = if nonsymbol {
        "42".to_owned()
    } else if positioned {
        format!("(position-symbol '{name} 0)")
    } else {
        format!("'{name}")
    };
    format!(
        "(mapcar (lambda (enabled)\n           (let* ((symbols-with-pos-enabled t) (arg {argument}))\n             (let ((symbols-with-pos-enabled enabled))\n               (condition-case err {call}\n                 (wrong-type-argument (car (cdr err))))))) '(nil t))"
    )
}

#[test]
fn issue554_oracle_charset_dispatch_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let call = r#"(progn (set-charset-priority arg) (eq (charset-priority-list t) 'ascii))"#;
    for (positioned, nonsymbol, expected) in [
        (true, false, "(charsetp t)"),
        (false, false, "(t t)"),
        (false, true, "(charsetp charsetp)"),
    ] {
        let (oracle, neomacs) =
            eval_oracle_and_neovm(&pair_form(call, "ascii", positioned, nonsymbol));
        assert_ok_eq(expected, &oracle, &neomacs);
    }
}

#[test]
fn issue554_oracle_charset_dispatch_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let call = r#"(= (charset-id-internal arg) (charset-id-internal 'ascii))"#;
    for (positioned, nonsymbol, expected) in [
        (true, false, "(charsetp t)"),
        (false, false, "(t t)"),
        (false, true, "(charsetp charsetp)"),
    ] {
        let (oracle, neomacs) =
            eval_oracle_and_neovm(&pair_form(call, "ascii", positioned, nonsymbol));
        assert_ok_eq(expected, &oracle, &neomacs);
    }
}

#[test]
fn issue554_oracle_charset_dispatch_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let call = r#"(progn (define-charset-internal arg 1 [0 127 0 0 0 0 0 0] nil nil nil nil nil t nil nil 0 nil nil nil nil nil) (charsetp 'issue554-dispatch-charset))"#;
    for (positioned, nonsymbol, expected) in [
        (true, false, "(symbolp t)"),
        (false, false, "(t t)"),
        (false, true, "(symbolp symbolp)"),
    ] {
        let (oracle, neomacs) = eval_oracle_and_neovm(&pair_form(
            call,
            "issue554-dispatch-charset",
            positioned,
            nonsymbol,
        ));
        assert_ok_eq(expected, &oracle, &neomacs);
    }
}

#[test]
fn issue554_oracle_charset_dispatch_equivalent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let call = r#"(null (declare-equiv-charset 1 94 ?A arg))"#;
    for (positioned, nonsymbol, expected) in [
        (true, false, "(charsetp t)"),
        (false, false, "(t t)"),
        (false, true, "(charsetp charsetp)"),
    ] {
        let (oracle, neomacs) =
            eval_oracle_and_neovm(&pair_form(call, "ascii", positioned, nonsymbol));
        assert_ok_eq(expected, &oracle, &neomacs);
    }
}

#[test]
fn issue554_oracle_charset_dispatch_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let call = r#"(progn (define-charset-alias (if enabled (position-symbol 'issue554-dispatch-alias 0) 'issue554-dispatch-alias) arg) (charsetp 'issue554-dispatch-alias))"#;
    for (positioned, nonsymbol, expected) in [
        (true, false, "(charsetp t)"),
        (false, false, "(t t)"),
        (false, true, "(charsetp charsetp)"),
    ] {
        let (oracle, neomacs) =
            eval_oracle_and_neovm(&pair_form(call, "ascii", positioned, nonsymbol));
        assert_ok_eq(expected, &oracle, &neomacs);
    }
}
