//! Divergence tests: register operations, point, marker deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_marker_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 8 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"Hello World\")
  (let ((m1 (make-marker))
        (m2 (make-marker)))
    (set-marker m1 3 (current-buffer))
    (set-marker m2 8 (current-buffer))
    (list (marker-position m1)
          (marker-position m2)
          (markerp m1)
          (markerp m2)
          (eq (marker-buffer m1) (current-buffer))))) ",
        expect,
    );
}

#[test]
fn divergence_marker_insertion_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (7 t \"HellXYo World\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"Hello World\")
  (let ((m (make-marker)))
    (set-marker m 5 (current-buffer))
    (set-marker-insertion-type m t)
    (goto-char 5)
    (insert \"XY\")
    (list (marker-position m)
          (marker-insertion-type m)
          (buffer-string)))) ",
        expect,
    );
}

#[test]
fn divergence_marker_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (7 \"ABXXCDEFGHIJ\" 69)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (let ((m (make-marker)))
    (set-marker m 5 (current-buffer))
    (goto-char 3)
    (insert \"XX\")
    (list (marker-position m)
          (buffer-string)
          (char-after m)))) ",
        expect,
    );
}

#[test]
fn divergence_marker_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 \"ABFGHIJ\" #<buffer *scratch*>)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (let ((m (make-marker)))
    (set-marker m 8 (current-buffer))
    (delete-region 3 6)
    (list (marker-position m)
          (buffer-string)
          (marker-buffer m)))) ",
        expect,
    );
}

#[test]
fn divergence_many_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function buffer-has-markers-at)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (let ((markers (mapcar (lambda (i) (let ((m (make-marker)))
                                     (set-marker m (1+ i) (current-buffer))
                                     m))
                         (number-sequence 0 9))))
    (list (mapcar 'marker-position markers)
          (length markers)
          (buffer-has-markers-at 1)
          (= (car (mapcar 'marker-position markers)) 1)))) ",
        expect,
    );
}

#[test]
fn divergence_point_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (14 3 \"XXHello World\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"Hello World\")
  (let ((pm (point-marker)))
    (goto-char 1)
    (insert \"XX\")
    (list (marker-position pm)
          (point)
          (buffer-string)))) ",
        expect,
    );
}

#[test]
fn divergence_region_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (fboundp 'region-beginning)
  (fboundp 'region-end)
  (fboundp 'use-region-p)
  (fboundp 'region-noncontiguous-p)
  (fboundp 'region-extract-function)) ",
        expect,
    );
}

#[test]
fn divergence_temporary_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (fboundp 'copy-marker)
  (fboundp 'set-marker)
  (fboundp 'move-marker)
  (fboundp 'marker-buffer)
  (fboundp 'marker-position)) ",
        expect,
    );
}

#[test]
fn divergence_field_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"Hello World\")
  (put-text-property 1 6 'field 'greeting)
  (list (fboundp 'field-beginning)
        (fboundp 'field-end)
        (fboundp 'field-string)
        (fboundp 'delete-field))) ",
        expect,
    );
}

#[test]
fn divergence_restriction_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (fboundp 'internal--char-to-point)
  (fboundp 'posnp)
  (fboundp 'posn-at-point)
  (fboundp 'posn-at-x-y)) ",
        expect,
    );
}
