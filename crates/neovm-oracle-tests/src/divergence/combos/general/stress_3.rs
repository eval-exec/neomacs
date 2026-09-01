//! Divergence tests: stress - nested eval, deep recursion, large data.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_deep_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (100 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((x 0))
  (dotimes (_ 100)
    (setq x (1+ x)))
  (list x (= x 100))) "#,
        expect,
    );
}

#[test]
fn divergence_nested_condition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (inner)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((result nil))
  (condition-case err
      (condition-case err2
          (signal 'wrong-type-argument '(test))
        (wrong-type-argument (push 'inner result)))
    (error (push 'outer result)))
  result) "#,
        expect,
    );
}

#[test]
fn divergence_nested_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((log nil))
  (unwind-protect
      (unwind-protect
          (progn (push 'body1 log) (error "test"))
        (push 'cleanup1 log))
    (push 'cleanup2 log))
  log) "#,
        expect,
    );
}

#[test]
fn divergence_large_list_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (100 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((lst (number-sequence 1 100)))
  (list (length lst)
        (= (car lst) 1)
        (= (car (last lst)) 100)
        (= (apply '+ lst) 5050))) "#,
        expect,
    );
}

#[test]
fn divergence_large_string_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1000 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s (make-string 1000 ?x)))
  (list (length s)
        (= (length s) 1000)
        (string= (substring s 0 5) "xxxxx")
        (string= (substring s -5) "xxxxx"))) "#,
        expect,
    );
}

#[test]
fn divergence_deep_let_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 40 55)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((a 1) (b 2) (c 3) (d 4) (e 5) (f 6) (g 7) (h 8) (i 9) (j 10))
  (list (+ a b c d e) (+ f g h i j) (+ a b c d e f g h i j))) "#,
        expect,
    );
}

#[test]
fn divergence_interleaved_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"123ABCDEFGHIJ456\" 17 16)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (goto-char 1)
  (insert "123")
  (goto-char (point-max))
  (insert "456")
  (list (buffer-string) (point) (buffer-size))) "#,
        expect,
    );
}

#[test]
fn divergence_many_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (dotimes (i 5)
    (make-overlay (1+ (* i 2)) (+ 2 (* i 2))))
  (list (length (overlays-in 1 11))
        (overlay-start (car (overlays-in 1 3))))) "#,
        expect,
    );
}

#[test]
fn divergence_many_text_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 nil 1 nil 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (dotimes (i 5)
    (put-text-property (1+ (* i 2)) (+ 2 (* i 2)) 'test-prop i))
  (list (get-text-property 1 'test-prop)
        (get-text-property 2 'test-prop)
        (get-text-property 3 'test-prop)
        (get-text-property 4 'test-prop)
        (get-text-property 5 'test-prop))) "#,
        expect,
    );
}

#[test]
fn divergence_stress_combo_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (#(\"llo Wo\" 0 3 (face bold)) bold 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (put-text-property 1 6 'face 'bold)
  (make-overlay 7 12)
  (narrow-to-region 3 9)
  (list (buffer-string)
        (get-text-property (point-min) 'face)
        (length (overlays-in (point-min) (point-max))))) "#,
        expect,
    );
}
