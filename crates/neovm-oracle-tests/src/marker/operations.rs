//! Oracle parity tests for marker operations: `point-marker`,
//! `copy-marker`, `marker-position`, `marker-buffer`,
//! `set-marker`, `marker-insertion-type`, and complex marker patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// point-marker / marker-position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (goto-char 6)
                    (let ((m (point-marker)))
                      (list (markerp m)
                            (marker-position m)
                            (eq (marker-buffer m) (current-buffer)))))"#;
    let expect = expect_test::expect![[r#""OK (t 6 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-marker
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (goto-char 6)
                    (let ((m1 (point-marker)))
                      (let ((m2 (copy-marker m1)))
                        (list (marker-position m1)
                              (marker-position m2)
                              (eq m1 m2)
                              (= (marker-position m1)
                                 (marker-position m2))))))"#;
    let expect = expect_test::expect![[r#""OK (6 6 nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-marker
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_set_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "0123456789")
                    (let ((m (make-marker)))
                      (set-marker m 5 (current-buffer))
                      (let ((pos1 (marker-position m)))
                        (set-marker m 8)
                        (let ((pos2 (marker-position m)))
                          ;; Unset marker
                          (set-marker m nil)
                          (list pos1 pos2
                                (marker-position m))))))"#;
    let expect = expect_test::expect![[r#""OK (5 8 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Marker movement with buffer changes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_moves_with_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "ABCDE")
                    (let ((m (copy-marker 3)))
                      (let ((before (marker-position m)))
                        ;; Insert before marker
                        (goto-char 2)
                        (insert "xx")
                        (let ((after-insert (marker-position m)))
                          ;; Delete before marker
                          (delete-region 1 3)
                          (list before
                                after-insert
                                (marker-position m)
                                (buffer-string))))))"#;
    let expect = expect_test::expect![[r#""OK (3 5 3 \"xBCDE\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// marker-insertion-type
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_insertion_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "ABCDE")
                    ;; Default: marker stays when text inserted at its position
                    (let ((m1 (copy-marker 3))
                          (m2 (copy-marker 3 t)))
                      (let ((t1 (marker-insertion-type m1))
                            (t2 (marker-insertion-type m2)))
                        ;; Insert at position 3
                        (goto-char 3)
                        (insert "xx")
                        (list t1 t2
                              (marker-position m1)
                              (marker-position m2)))))"#;
    let expect = expect_test::expect![[r#""OK (nil t 3 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: bracket matching with markers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_bracket_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track positions of matching brackets
    let form = r#"(with-temp-buffer
                    (insert "(a (b c) (d (e) f) g)")
                    (goto-char (point-min))
                    (let ((stack nil)
                          (pairs nil))
                      (while (< (point) (point-max))
                        (let ((c (char-after (point))))
                          (cond
                           ((= c ?\()
                            (setq stack (cons (copy-marker (point)) stack)))
                           ((= c ?\))
                            (when stack
                              (let ((open-marker (car stack)))
                                (setq pairs
                                      (cons (list (marker-position open-marker)
                                                  (point))
                                            pairs))
                                (setq stack (cdr stack)))))))
                        (forward-char 1))
                      (nreverse pairs)))"#;
    let expect = expect_test::expect![[r#""OK ((4 8) (13 15) (10 18) (1 21))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multiple markers surviving edits
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_marker_survive_edits() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "line1\nline2\nline3\nline4\n")
                    ;; Place markers at start of each line
                    (goto-char (point-min))
                    (let ((markers nil))
                      (while (not (eobp))
                        (setq markers
                              (cons (point-marker) markers))
                        (forward-line 1))
                      (setq markers (nreverse markers))
                      ;; Record initial positions
                      (let ((before (mapcar #'marker-position markers)))
                        ;; Insert text at beginning
                        (goto-char (point-min))
                        (insert "HEADER\n")
                        ;; Record positions after insertion
                        (let ((after (mapcar #'marker-position markers)))
                          (list before after
                                (buffer-string))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 7 13 19) (1 14 20 26) \"HEADER\\nline1\\nline2\\nline3\\nline4\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
