//! Divergence tests: minibuffer, completion, and ring data structure.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_minibuffer_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable minibuf-window)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (booleanp minibuf-window)
  (windowp minibuf-window)
  (integerp minibuffer-depth-indicator-function)
  (booleanp enable-recursive-minibuffers))"#,
        expect,
    );
}

#[test]
fn divergence_minibuffer_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (windowp (minibuffer-window))
  (window-live-p (minibuffer-window))
  (bufferp (window-buffer (minibuffer-window)))
  (minibuffer-window-active-p (minibuffer-window))
  (window-minibuffer-p (minibuffer-window)))"#,
        expect,
    );
}

#[test]
fn divergence_minibuffer_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (stringp (minibuffer-prompt-width))
  (fboundp 'minibuffer-contents)
  (fboundp 'minibuffer-contents-no-properties))"#,
        expect,
    );
}

#[test]
fn divergence_completion_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'try-completion)
  (fboundp 'all-completions)
  (fboundp 'completion-boundaries)
  (string= (try-completion "fo" '(("foo") ("bar") ("foobar"))) "foobar"))"#,
        expect,
    );
}

#[test]
fn divergence_all_completions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"alpha\" \"alphabetic\") \"alpha\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((coll '(("alpha") ("beta") ("gamma") ("alphabetic"))))
  (list (sort (all-completions "al" coll) #'string<)
        (try-completion "al" coll)))"#,
        expect,
    );
}

#[test]
fn divergence_completion_regexp_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"alpha\") (\"lph\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((completion-regexp-list '("lph")))
  (list (all-completions "" '(("alpha") ("beta") ("gamma")))
        completion-regexp-list))"#,
        expect,
    );
}

#[test]
fn divergence_ring_create_and_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (c b a 3 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'ring)
(let ((r (make-ring 5)))
  (ring-insert r 'a)
  (ring-insert r 'b)
  (ring-insert r 'c)
  (list (ring-ref r 0)
        (ring-ref r 1)
        (ring-ref r 2)
        (ring-length r)
        (ring-size r)))"#,
        expect,
    );
}

#[test]
fn divergence_ring_overflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 3 2 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'ring)
(let ((r (make-ring 3)))
  (dotimes (i 5)
    (ring-insert r i))
  (list (ring-ref r 0)
        (ring-ref r 1)
        (ring-ref r 2)
        (ring-length r)
        (ring-size r)))"#,
        expect,
    );
}

#[test]
fn divergence_ring_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (c a 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'ring)
(let ((r (make-ring 5)))
  (ring-insert r 'a)
  (ring-insert r 'b)
  (ring-insert r 'c)
  (ring-remove r 1)
  (list (ring-ref r 0)
        (ring-ref r 1)
        (ring-length r)))"#,
        expect,
    );
}

#[test]
fn divergence_ring_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((5 1 4 1 3) (0 5 . [3 1 4 1 5]))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'ring)
(let ((r (make-ring 5)))
  (ring-insert r 3)
  (ring-insert r 1)
  (ring-insert r 4)
  (ring-insert r 1)
  (ring-insert r 5)
  (list (ring-elements r)
        (ring-copy r)))"#,
        expect,
    );
}
