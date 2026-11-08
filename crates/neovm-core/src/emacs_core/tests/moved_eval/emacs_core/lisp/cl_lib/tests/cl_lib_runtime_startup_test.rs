//! cl-lib tests whose harness is `runtime_startup_eval_all`.
//!
//! That helper loads the byte-compiled startup image. The proof is minted
//! in core, so eval cannot construct it. The direct builtin tests stay in
//! eval. Assertions are unchanged.

use crate::test_utils::runtime_startup_eval_all;

fn bootstrap_eval(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

#[test]
fn cl_subseq_list() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (progn (require 'cl-lib) (cl-subseq '(a b c) 1 3))
        (progn (require 'cl-lib) (cl-concatenate 'list '(a b) '(c)))
        (progn (require 'cl-lib) (cl-remove-duplicates '(a b a c b)))
        "#,
    );
    assert_eq!(results[0], "OK (b c)");
    assert_eq!(results[1], "OK (a b c)");
    assert_eq!(results[2], "OK (a c b)");
}

#[test]
fn cl_subseq_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (condition-case err
            (progn (require 'cl-lib) (cl-subseq 0))
          (wrong-number-of-arguments (car err)))
        (condition-case err
            (progn (require 'cl-lib) (cl-concatenate 0 nil))
          (error (car err)))
        (condition-case err
            (progn (require 'cl-lib) (cl-remove-duplicates nil nil))
          (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK wrong-number-of-arguments");
    assert_eq!(results[1], "OK error");
    assert_eq!(results[2], "OK error");
}

#[test]
fn cl_subseq_wrong_type() {
    crate::test_utils::init_test_tracing();
    // GNU signals (error "Unsupported sequence: 0"), not wrong-type-argument.
    // Verified against GNU Emacs --batch.
    let results = bootstrap_eval(
        r#"
        (condition-case err
            (progn (require 'cl-lib) (cl-subseq 0 0))
          (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK error");
}

#[test]
fn cl_reduce_with_eval() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (progn (require 'cl-lib) (cl-reduce #'+ '(1 2 3) :initial-value 0))
        (progn (require 'cl-lib) (cl-count 1 '(1 "x" 1)))
        (progn (require 'cl-lib) (cl-count-if #'numberp '(1 "x" 2)))
        (progn (require 'cl-lib) (cl-some #'numberp '("x" 2)))
        (progn (require 'cl-lib) (cl-every #'numberp '(1 2 3)))
        "#,
    );
    assert_eq!(results[0], "OK 6");
    assert_eq!(results[1], "OK 2");
    assert_eq!(results[2], "OK 2");
    assert_eq!(results[3], "OK t");
    assert_eq!(results[4], "OK t");
}

#[test]
fn cl_reduce_without_initial_value_bootstrap() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (progn (require 'cl-lib) (cl-reduce #'+ '(1 2 3)))
        (progn (require 'cl-lib) (cl-reduce #'+ '(42)))
        "#,
    );
    assert_eq!(results[0], "OK 6");
    assert_eq!(results[1], "OK 42");
}

#[test]
fn cl_count_some_every_bootstrap() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (progn (require 'cl-lib) (cl-count 1 '(1 "x" 1)))
        (progn (require 'cl-lib) (cl-count-if #'numberp '(1 "x" 2)))
        (progn (require 'cl-lib) (cl-some #'numberp '("x" 2)))
        (progn (require 'cl-lib) (cl-every #'numberp '(1 2 3)))
        "#,
    );
    assert_eq!(results[0], "OK 2");
    assert_eq!(results[1], "OK 2");
    assert_eq!(results[2], "OK t");
    assert_eq!(results[3], "OK t");
}
