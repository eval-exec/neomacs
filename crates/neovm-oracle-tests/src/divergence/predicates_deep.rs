//! Divergence tests: misc predicates, type-checking, equality edge.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_type_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (integer string cons vector symbol integer)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (type-of 42)
  (type-of "hello")
  (type-of '(1 2))
  (type-of [1 2])
  (type-of 'foo)
  (type-of ?A)) "#,
        expect,
    );
}

#[test]
fn divergence_equality_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (equal '(1 2 3) '(1 2 3))
  (equal [1 2 3] [1 2 3])
  (eq 'foo 'foo)
  (eql 42 42)
  (equal-including-properties "abc" "abc")) "#,
        expect,
    );
}

#[test]
fn divergence_number_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t nil t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (numberp 42)
  (numberp 3.14)
  (numberp "string")
  (integerp 42)
  (integerp 3.14)
  (floatp 3.14)
  (floatp 42)
  (natnump 5)
  (natnump -1)) "#,
        expect,
    );
}

#[test]
fn divergence_sequence_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (sequencep '(1 2 3))
  (sequencep [1 2 3])
  (sequencep "abc")
  (listp '(1 2))
  (listp nil)
  (consp '(1 2))
  (arrayp [1 2])
  (arrayp "abc")
  (stringp "abc")) "#,
        expect,
    );
}

#[test]
fn divergence_nil_t_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (null nil)
  (null t)
  (not nil)
  (not t)
  (booleanp nil)
  (booleanp t)
  (booleanp 0)) "#,
        expect,
    );
}

#[test]
fn divergence_char_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (characterp ?A)
  (characterp 128)
  (characterp #x4e2d)
  (characterp ?\n)
  (wholenump 5)
  (wholenump -1)) "#,
        expect,
    );
}

#[test]
fn divergence_function_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (functionp 'car)
  (functionp 'lambda)
  (functionp 42)
  (subrp (symbol-function 'car))
  (byte-code-function-p (symbol-function 'car))
  (commandp 'car)) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (bufferp (current-buffer))
  (bufferp nil)
  (buffer-live-p (current-buffer))
  (buffer-modified-p (current-buffer))
  (buffer-file-name (current-buffer))) "#,
        expect,
    );
}

#[test]
fn divergence_marker_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil #<marker at 1 in *scratch*> 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((m (make-marker)))
  (list (markerp m)
        (marker-position m)
        (marker-buffer m)
        (set-marker m 1 (current-buffer))
        (marker-position m)
        (markerp 42))) "#,
        expect,
    );
}

#[test]
fn divergence_window_frame_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (windowp (selected-window))
  (window-live-p (selected-window))
  (window-valid-p (selected-window))
  (framep (selected-frame))
  (frame-live-p (selected-frame))) "#,
        expect,
    );
}

#[test]
fn divergence_process_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'processp)
  (processp nil)
  (fboundp 'process-live-p)) "#,
        expect,
    );
}
