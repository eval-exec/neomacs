//! GNU byte-code is an ordinary redefinable function whose arguments evaluate.
use crate::emacs_core::{eval::Context, print::print_value};

#[test]
fn p6_byte_code_evaluates_its_string_and_depth_arguments() {
    let mut context = Context::new();
    let result = context
        .eval_str(r#"(let ((code "\300\207") (depth 1)) (byte-code code [42] depth))"#)
        .unwrap();
    assert_eq!(print_value(&result), "42");
    // GNU byte-code accepts a legacy multibyte code string and converts its
    // characters to the original byte program before executing it.
    let legacy = context
        .eval_str(r#"(byte-code (string-as-multibyte (unibyte-string 192 135)) [42] 1)"#)
        .unwrap();
    assert_eq!(print_value(&legacy), "42");
}

#[test]
fn p6_byte_code_rejects_invalid_code_and_noninteger_depth() {
    let mut context = Context::new();
    for form in [
        r#"(byte-code 7 [42] 1)"#,
        r#"(byte-code "\300\207" [42] 1.0)"#,
    ] {
        let result = context
            .eval_str(&format!("(condition-case err {form} (error err))"))
            .unwrap();
        assert_eq!(print_value(&result), r#"(error "Invalid byte-code")"#);
    }
}

#[test]
fn p6_byte_code_obeys_function_cell_redefinition() {
    let mut context = Context::new();
    let result = context.eval_str("(progn (fset 'byte-code (lambda (a b c) (list a b c))) (byte-code (+ 1 2) (+ 3 4) (+ 5 6)))").unwrap();
    assert_eq!(print_value(&result), "(3 7 11)");
}

#[test]
fn p6_byte_code_keeps_its_constants_alive_across_a_collection() {
    let mut context = Context::new();
    // GNU opcodes: Bconstant1, Bcall0, Bdiscard, Bconstant0, Breturn. The
    // function GNU Fbyte_code builds is never named by Lisp; it lives only
    // for this call, and its constants must survive the collection it runs.
    let result = context
        .eval_str(
            r#"(byte-code "\301\040\210\300\207"
                          (vector (list 1 2 (make-string 3 ?x)) 'garbage-collect)
                          2)"#,
        )
        .unwrap();
    assert_eq!(print_value(&result), r#"(1 2 "xxx")"#);
}
