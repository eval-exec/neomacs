//! Complex combo batch 394 — `completion`/`minibuffer` ultimate:
//! try-completion, all-completions, test-completion, completion-styles,
//! completing-read availability, minibuffer-depth/window/keymaps.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx394_try_completion_all_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"alp\" \"alpha\" t nil \"a\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '("alpha" "alphabet" "alpine" "amplitude" "antelope" "beta" "gamma")))
  (list (try-completion "al" coll)
        (try-completion "alph" coll)
        (try-completion "alphabet" coll)
        (try-completion "z" coll)
        (try-completion "a" coll)))
"##,
        expect,
    )
}

#[test]
fn div_cx394_all_completions_with_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"apple1\" \"apple2\") (\"banana1\" \"banana2\") (\"apple1\") (\"banana2\") \"apple\" 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '("apple1" "apple2" "banana1" "banana2" "cherry1" "cherry2")))
  (list (all-completions "a" coll)
        (all-completions "b" coll)
        (all-completions "a" coll (lambda (s) (string-match-p "1$" s)))
        (all-completions "b" coll (lambda (s) (string-match-p "2$" s)))
        (try-completion "app" coll)
        (length (all-completions "" coll))))
"##,
        expect,
    )
}

#[test]
fn div_cx394_test_completion_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '("alpha" "alphabet" "alpine" "beta" "gamma" "delta")))
  (list (test-completion "alpha" coll)
      (test-completion "alp" coll)
      (test-completion "missing" coll)
      (test-completion "gamma" coll)))
"##,
        expect,
    )
}

#[test]
fn div_cx394_completion_with_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ob (make-obarray 31)))
  (intern "alpha" ob)
  (intern "alphabet" ob)
  (intern "alpine" ob)
  (intern "amplitude" ob)
  (list (all-completions "al" ob)
        (all-completions "amp" ob)
        (try-completion "al" ob)))
"##,
        expect,
    )
}

#[test]
fn div_cx394_completion_ignore_case_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3 \"alpha\" \"Alpha\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '("Alpha" "ALPHA" "alpha" "Beta"))
      (completion-ignore-case t))
  (list (length (all-completions "a" coll))
        (length (all-completions "A" coll))
        (try-completion "a" coll)
        (try-completion "A" coll)))
"##,
        expect,
    )
}

#[test]
fn div_cx394_completion_styles_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((basic (basic partial-completion emacs22)) (partial-completion (partial-completion emacs22)) (substring nil) (initials nil) (emacs22 (emacs22)) (flex nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((styles '(basic partial-completion substring initials emacs22
                  flex)))
  (mapcar (lambda (s) (list s (memq s completion-styles))) styles))
"##,
        expect,
    )
}

#[test]
fn div_cx394_completion_table_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"alp\" (\"alpha\" \"alphabet\" \"alpine\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((dynamic-table
       (completion-table-dynamic
        (lambda (str)
          (all-completions str '("alpha" "alphabet" "alpine"))))))
  (list (try-completion "al" dynamic-table)
        (all-completions "al" dynamic-table)))
"##,
        expect,
    )
}

#[test]
fn div_cx394_completion_table_in_turn_and_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((combined (completion-table-merge
                     '("alpha" "alphabet")
                     '("amplitude" "antelope"))))
      (list (try-completion "a" combined)
            (length (all-completions "a" combined))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx394_minibuffer_availability_and_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'read-from-minibuffer)
      (fboundp 'read-string)
      (fboundp 'completing-read)
      (fboundp 'read-char)
      (fboundp 'read-event)
      (fboundp 'read-key)
      (>= (minibuffer-depth) 0)
      (boundp 'enable-recursive-minibuffers)
      (boundp 'minibuffer-local-map)
      (keymapp minibuffer-local-map))
"##,
        expect,
    )
}

#[test]
fn div_cx394_completion_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '("alpha" "alphabet" "alpine" "amplitude" "antelope")))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (mapconcat #'identity (all-completions "a" coll) " "))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list (try-completion "al" coll)
                         (all-completions "al" coll)
                         (test-completion "alpha" coll)
                         (length (all-completions "" coll))
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen()
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
