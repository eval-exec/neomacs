//! Builtins-extra tests whose harness is `runtime_startup_eval_all`.
//!
//! That helper loads the byte-compiled startup image. The proof is minted
//! in core, so eval cannot construct it. The direct builtin tests stay in
//! eval. Assertions are unchanged.

use crate::test_utils::runtime_startup_eval_all;

fn bootstrap_eval(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

#[test]
fn remove_family_bootstrap_matches_gnu_subr() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (subrp (symbol-function 'remove))
        (subrp (symbol-function 'remq))
        (subrp (symbol-function 'flatten-tree))
        (remove 2 '(1 2 3 2))
        (remq 'a '(a b a c))
        (flatten-tree '(1 (2 . 3) nil (4 5 (6)) 7))
        "#,
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], "OK (1 3)");
    assert_eq!(results[4], "OK (b c)");
    assert_eq!(results[5], "OK (1 2 3 4 5 6 7)");
}

#[test]
fn string_empty_blank() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (string-empty-p "")
        (string-empty-p "a")
        (string-blank-p "  ")
        (string-blank-p "x")
        "#,
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK 0");
    assert_eq!(results[3], "OK nil");
}

#[test]
fn string_replace_bootstrap_matches_gnu_subr() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (subrp (symbol-function 'string-replace))
        (string-replace "world" "rust" "hello world")
        (string-replace "x" "y" "no match")
        (condition-case err (string-replace "" "-" "abc") (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], r#"OK "hello rust""#);
    assert_eq!(results[2], r#"OK "no match""#);
    // wrong-length-argument is a subtype of error, so condition-case
    // catches it and (car err) returns the error symbol.
    assert_eq!(results[3], "OK wrong-length-argument");
}

#[test]
fn fixnum_predicates_bootstrap_match_gnu_subr() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (subrp (symbol-function 'fixnump))
        (subrp (symbol-function 'bignump))
        (list (fixnump 0)
              (fixnump most-positive-fixnum)
              (fixnump 1.0)
              (fixnump nil))
        (list (bignump 0)
              (bignump most-positive-fixnum)
              (bignump 1.0)
              (bignump nil))
        "#,
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK (t t nil nil)");
    assert_eq!(results[3], "OK (nil nil nil nil)");
}

#[test]
fn seq_uniq() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (seq-uniq '(1 2 1 3))
        (seq-uniq '("Hello" "hello" "HELLO") #'string-equal-ignore-case)
        "#,
    );
    assert_eq!(results[0], "OK (1 2 3)");
    assert_eq!(results[1], "OK (\"Hello\")");
}

#[test]
fn seq_length_list_and_string() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (seq-length '(1 2 3))
        (seq-length "hello")
        (seq-into '(1 2 3) 'vector)
        (seq-into [?h ?i] 'string)
        "#,
    );
    assert_eq!(results[0], "OK 3");
    assert_eq!(results[1], "OK 5");
    assert_eq!(results[2], "OK [1 2 3]");
    assert_eq!(results[3], "OK \"hi\"");
}

#[test]
fn seq_length_wrong_type_errors() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (condition-case err
            (seq-length 42)
          (wrong-type-argument (car err)))
        (condition-case err
            (seq-into '(1 2 3) 'hash-table)
          (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK wrong-type-argument");
    assert_eq!(results[1], "OK error");
}

#[test]
fn user_full_name_no_arg_reads_current_special_variable() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (setq user-full-name "Alice")
        (user-full-name)
        (let ((user-full-name "Bob"))
          (user-full-name))
        "#,
    );
    assert_eq!(results[0], "OK \"Alice\"");
    assert_eq!(results[1], "OK \"Alice\"");
    assert_eq!(results[2], "OK \"Bob\"");
}
