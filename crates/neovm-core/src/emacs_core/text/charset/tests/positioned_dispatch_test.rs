//! Exercise the registered Lisp dispatcher, not just charset helper adapters.
use crate::emacs_core::{eval::Context, print::print_value};

fn answer(source: &str) -> String {
    let mut context = Context::new();
    print_value(
        &context
            .eval_str(source)
            .expect("positioned charset dispatch"),
    )
}

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
fn positioned_charset_dispatch_priority() {
    let call = r#"(progn (set-charset-priority arg) (eq (charset-priority-list t) 'ascii))"#;
    assert_eq!(
        answer(&pair_form(call, "ascii", true, false)),
        "(charsetp t)"
    );
    assert_eq!(answer(&pair_form(call, "ascii", false, false)), "(t t)");
    assert_eq!(
        answer(&pair_form(call, "ascii", false, true)),
        "(charsetp charsetp)"
    );
}

#[test]
fn positioned_charset_dispatch_id() {
    let call = r#"(= (charset-id-internal arg) (charset-id-internal 'ascii))"#;
    assert_eq!(
        answer(&pair_form(call, "ascii", true, false)),
        "(charsetp t)"
    );
    assert_eq!(answer(&pair_form(call, "ascii", false, false)), "(t t)");
    assert_eq!(
        answer(&pair_form(call, "ascii", false, true)),
        "(charsetp charsetp)"
    );
}

#[test]
fn positioned_charset_dispatch_definition() {
    let call = r#"(progn (define-charset-internal arg 1 [0 127 0 0 0 0 0 0] nil nil nil nil nil t nil nil 0 nil nil nil nil nil) (charsetp 'issue554-dispatch-charset))"#;
    assert_eq!(
        answer(&pair_form(call, "issue554-dispatch-charset", true, false)),
        "(symbolp t)"
    );
    assert_eq!(
        answer(&pair_form(call, "issue554-dispatch-charset", false, false)),
        "(t t)"
    );
    assert_eq!(
        answer(&pair_form(call, "issue554-dispatch-charset", false, true)),
        "(symbolp symbolp)"
    );
}

#[test]
fn positioned_charset_dispatch_equivalent() {
    let call = r#"(null (declare-equiv-charset 1 94 ?A arg))"#;
    assert_eq!(
        answer(&pair_form(call, "ascii", true, false)),
        "(charsetp t)"
    );
    assert_eq!(answer(&pair_form(call, "ascii", false, false)), "(t t)");
    assert_eq!(
        answer(&pair_form(call, "ascii", false, true)),
        "(charsetp charsetp)"
    );
}

#[test]
fn positioned_charset_dispatch_alias() {
    let call = r#"(progn (define-charset-alias (if enabled (position-symbol 'issue554-dispatch-alias 0) 'issue554-dispatch-alias) arg) (charsetp 'issue554-dispatch-alias))"#;
    assert_eq!(
        answer(&pair_form(call, "ascii", true, false)),
        "(charsetp t)"
    );
    assert_eq!(answer(&pair_form(call, "ascii", false, false)), "(t t)");
    assert_eq!(
        answer(&pair_form(call, "ascii", false, true)),
        "(charsetp charsetp)"
    );
}
