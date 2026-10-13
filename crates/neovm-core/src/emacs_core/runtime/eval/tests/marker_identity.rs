//! GNU eval.c3405/3412 and3570/3572 compare canonical marker identities.
use crate::emacs_core::{eval::Context, print::print_value};

#[test]
fn p6_uninterned_parameter_marker_names_bind_as_required_variables() {
    let mut context = Context::new();
    let mut answers = Vec::new();
    for name in ["&optional", "&rest"] {
        let source = format!(
            r#"(let* ((formal (make-symbol "{name}"))
               (function (make-byte-code (list formal) (unibyte-string 8 135) (vector formal) 1)))
          (list (func-arity function) (funcall function 42)
                (condition-case error (funcall function) (error (car error)))))"#
        );
        answers.push(print_value(&context.eval_str(&source).unwrap()));
    }
    assert_eq!(
        answers,
        [
            "((1 . 1) 42 wrong-number-of-arguments)",
            "((1 . 1) 42 wrong-number-of-arguments)"
        ]
    );
}
