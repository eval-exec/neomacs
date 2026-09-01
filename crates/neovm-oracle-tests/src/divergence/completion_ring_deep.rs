//! Divergence tests: minibuffer history, ring operations, completion deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_ring_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 c b a 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ring (make-ring 5)))
  (ring-insert ring 'a)
  (ring-insert ring 'b)
  (ring-insert ring 'c)
  (list (ring-length ring)
        (ring-ref ring 0)
        (ring-ref ring 1)
        (ring-ref ring 2)
        (ring-size ring))) "#,
        expect,
    );
}

#[test]
fn divergence_ring_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 b a (b a))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ring (make-ring 5)))
  (ring-insert ring 'a)
  (ring-insert ring 'b)
  (ring-insert ring 'c)
  (ring-remove ring 0)
  (list (ring-length ring)
        (ring-ref ring 0)
        (ring-ref ring 1)
        (ring-elements ring))) "#,
        expect,
    );
}

#[test]
fn divergence_ring_overflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 3 d (d c b))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ring (make-ring 3)))
  (ring-insert ring 'a)
  (ring-insert ring 'b)
  (ring-insert ring 'c)
  (ring-insert ring 'd)
  (list (ring-length ring)
        (ring-size ring)
        (ring-ref ring 0)
        (ring-elements ring))) "#,
        expect,
    );
}

#[test]
fn divergence_minibuffer_history() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'minibuffer-history)
  (listp minibuffer-history)
  (boundp 'file-name-history)
  (listp file-name-history)
  (fboundp 'add-to-history)) "#,
        expect,
    );
}

#[test]
fn divergence_history_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'history-length)
  (integerp history-length)
  (boundp 'history-delete-duplicates)
  (booleanp history-delete-duplicates)) "#,
        expect,
    );
}

#[test]
fn divergence_completion_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'try-completion)
  (fboundp 'all-completions)
  (fboundp 'test-completion)
  (fboundp 'completion-boundaries))"#,
        expect,
    );
}

#[test]
fn divergence_completion_try() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((coll '(\"apple\" \"apricot\" \"banana\" \"cherry\")))
  (list (try-completion "ap" coll)
        (try-completion "b" coll)
        (try-completion "z" coll)
        (all-completions "ap" coll)
        (test-completion "apple" coll)
        (test-completion "appl" coll))) "#,
        expect,
    );
}

#[test]
fn divergence_completion_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"car\" \"calc-eval\" \"calendar-hebrew-list-yahrzeits\" \"canadian-aboriginal\" \"case-fold-search\" \"catch\" \"cancel-debug-on-entry\" \"call-pos\" \"capitalize\" \"calculate-lisp-indent\" \"category-table-p\" \"category-table\" \"calendar-bahai-all-holidays-flag\" \"cadddr\" \"caadar\" \"canonicalize-coding-system-name\" \"cari\" \"cancel-debug-watch\" \"caddar\" \"caaddr\" \"case-replace\") nil (\"catch\" \"cancel-debug-on-entry\" \"call-pos\" \"capitalize\" \"calculate-lisp-indent\" \"category-table-p\" \"category-table\" \"calendar-bahai-all-holidays-flag\" \"cadddr\" \"caadar\" \"canonicalize-coding-system-name\" \"cari\" \"cancel-debug-watch\" \"caddar\" \"caaddr\" \"case-replace\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((completions (all-completions "ca" obarray)))
  (list (member "car" completions)
        (member "cdr" completions)
        (member "catch" completions)
        (listp completions))) "#,
        expect,
    );
}

#[test]
fn divergence_completion_ignore_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((coll '(\"Hello\" \"HELLO\" \"hello\")))
  (list (try-completion "hel" coll)
        (try-completion "HEL" coll)
        (all-completions "hel" coll)
        (all-completions "HEL" coll))) "#,
        expect,
    );
}

#[test]
fn divergence_completion_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'completion-metadata)
  (fboundp 'completion-try-completion)
  (fboundp 'completion-all-completions)
  (fboundp 'completion--field-completion-function)) "#,
        expect,
    );
}
