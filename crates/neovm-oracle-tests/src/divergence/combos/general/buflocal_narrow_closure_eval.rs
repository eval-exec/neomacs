//! Divergence tests: buflocal + narrowing + closure + eval deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_buflocal_narrow_eval_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-blne-xxx 0)
  (make-variable-buffer-local 'test-blne-xxx)
  (setq test-blne-xxx 42)
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (let ((closure (let ((test-blne-xxx 100))
                   (lambda () test-blne-xxx))))
    (narrow-to-region 5 20)
    (let ((v1 (funcall closure))
          (v2 (eval 'test-blne-xxx))
          (v3 test-blne-xxx))
      (widen)
      (let ((v4 (funcall closure))
            (v5 (eval 'test-blne-xxx))
            (v6 test-blne-xxx))
        (list v1 v2 v3 v4 v5 v6
              (= v1 100)
              (= v2 42)
              (= v3 42)
              (= v4 100)
              (= v5 42)
              (= v6 42))))) "#,
        expect,
    );
}

#[test]
fn divergence_let_shadow_buflocal_eval_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 t (20 t 30 t) t 10 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-lsb-xxx 0)
  (make-variable-buffer-local 'test-lsb-xxx)
  (setq test-lsb-xxx 10)
  (list (eval 'test-lsb-xxx)
        (= (eval 'test-lsb-xxx) 10)
        (let ((test-lsb-xxx 20))
          (list (eval 'test-lsb-xxx)
                (= (eval 'test-lsb-xxx) 20)
                (let ((test-lsb-xxx 30))
                  (eval 'test-lsb-xxx))
                (= (eval 'test-lsb-xxx) 20)))
        (= (eval 'test-lsb-xxx) 10)
        test-lsb-xxx
        (= test-lsb-xxx 10))) "#,
        expect,
    );
}

#[test]
fn divergence_buflocal_kill_buffer_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 99 t t 10 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-bkr-xxx 0)
  (make-variable-buffer-local 'test-bkr-xxx)
  (setq test-bkr-xxx 10)
  (let ((buf1 (current-buffer))
        (buf2 (generate-new-buffer " test-bkr-xxx")))
    (with-current-buffer buf2
      (setq test-bkr-xxx 99))
    (let ((v1 (with-current-buffer buf1 test-bkr-xxx))
          (v2 (with-current-buffer buf2 test-bkr-xxx)))
      (kill-buffer buf2)
      (list v1 v2
            (= v1 10) (= v2 99)
            test-bkr-xxx
            (= test-bkr-xxx 10))))) "#,
        expect,
    );
}

#[test]
fn divergence_closure_over_buflocal_with_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-cbm-xxx 0)
  (make-variable-buffer-local 'test-cbm-xxx)
  (setq test-cbm-xxx 5)
  (insert "ABCDEFGHIJ")
  (let ((m (copy-marker 5 t)))
    (let ((reader (let ((test-cbm-xxx 100))
                    (lambda ()
                      (list test-cbm-xxx
                            (marker-position m))))))
      (goto-char 3)
      (insert "XX")
      (let ((result (funcall reader)))
        (list result
              (= (car result) 100)
              (> (cadr result) 5)
              test-cbm-xxx
              (= test-cbm-xxx 5)
              (marker-position m)
              (> (marker-position m) 5))))) "#,
        expect,
    );
}

#[test]
fn divergence_default_value_vs_buflocal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-dvb-xxx 42)
  (make-variable-buffer-local 'test-dvb-xxx)
  (setq test-dvb-xxx 100)
  (let ((buf2 (generate-new-buffer " test-dvb2-xxx")))
    (with-current-buffer buf2
      (setq test-dvb-xxx 200))
    (let ((default (default-value 'test-dvb-xxx)))
      (let ((result (list (symbol-value 'test-dvb-xxx)
                          default
                          (with-current-buffer buf2 test-dvb-xxx))))
        (kill-buffer buf2)
        (list result
              (= (car result) 100)
              (numberp default)
              (= (nth 2 result) 200))))) "#,
        expect,
    );
}

#[test]
fn divergence_narrowed_eval_with_closures_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-nen-xxx 'outer)
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
  (narrow-to-region 5 25)
  (let ((inner (let ((test-nen-xxx 'inner))
                 (lambda ()
                   (list test-nen-xxx (eval 'test-nen-xxx))))))
    (let ((result1 (funcall inner))
          (v1 (eval 'test-nen-xxx)))
      (widen)
      (let ((result2 (funcall inner))
            (v2 (eval 'test-nen-xxx)))
        (list result1 v1 result2 v2
              (eq (car result1) 'inner)
              (eq v1 'outer)
              (eq (car result2) 'inner)
              (eq v2 'outer))))) "#,
        expect,
    );
}

#[test]
fn divergence_buflocal_with_set_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-sd-xxx 0)
  (make-variable-buffer-local 'test-sd-xxx)
  (setq test-sd-xxx 10)
  (let ((buf2 (generate-new-buffer " test-sd2-xxx")))
    (with-current-buffer buf2
      (setq test-sd-xxx 20))
    (set-default 'test-sd-xxx 999)
    (let ((v1 test-sd-xxx)
          (v2 (with-current-buffer buf2 test-sd-xxx))
          (dv (default-value 'test-sd-xxx)))
      (kill-buffer buf2)
      (list v1 v2 dv
            (= v1 10)
            (= v2 20)
            (= dv 999)))) "#,
        expect,
    );
}

#[test]
fn divergence_closure_capture_with_eval_redefinition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-cer-xxx 1)
  (let ((closure (let ((test-cer-xxx 50))
                   (lambda () test-cer-xxx))))
    (list (funcall closure)
          (= (funcall closure) 50)
          (eval '(setq test-cer-xxx 999))
          (funcall closure)
          (= (funcall closure) 50)
          test-cer-xxx
          (= test-cer-xxx 999)
          (eval 'test-cer-xxx)
          (= (eval 'test-cer-xxx) 999))) "#,
        expect,
    );
}

#[test]
fn divergence_buflocal_with_undo_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable test-bui-xyz)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-bui-xxx 0)
  (make-variable-buffer-local 'test-bui-xxx)
  (setq test-bui-xxx 42)
  (insert "HELLO")
  (let ((ov (make-overlay 1 6)))
    (overlay-put ov 'val test-bui-xxx)
    (undo-boundary)
    (goto-char 3)
    (insert "XX")
    (let ((v1 test-bui-xxx)
          (ov-val (overlay-get ov 'val)))
      (primitive-undo 1 buffer-undo-list)
      (list v1 ov-val
            (= v1 42)
            (= ov-val 42)
            test-bui-xxx
            (= test-bui-xyz 42)
            (buffer-string)
            (overlay-get ov 'val)
            (= (overlay-get ov 'val) 42))))) "#,
        expect,
    );
}

#[test]
fn divergence_multiple_buflocal_vars_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-mbi-a-xxx 0)
  (defvar test-mbi-b-xxx 0)
  (defvar test-mbi-c-xxx 0)
  (make-variable-buffer-local 'test-mbi-a-xxx)
  (make-variable-buffer-local 'test-mbi-b-xxx)
  (make-variable-buffer-local 'test-mbi-c-xxx)
  (setq test-mbi-a-xxx 1
        test-mbi-b-xxx 2
        test-mbi-c-xxx 3)
  (let ((buf2 (generate-new-buffer " test-mbi2-xxx")))
    (with-current-buffer buf2
      (setq test-mbi-a-xxx 10
            test-mbi-b-xxx 20
            test-mbi-c-xxx 30))
    (let ((sum1 (+ test-mbi-a-xxx test-mbi-b-xxx test-mbi-c-xxx))
          (sum2 (with-current-buffer buf2
                  (+ test-mbi-a-xxx test-mbi-b-xxx test-mbi-c-xxx))))
      (kill-buffer buf2)
      (list sum1 sum2
            (= sum1 6)
            (= sum2 60)
            test-mbi-a-xxx test-mbi-b-xxx test-mbi-c-xxx
            (= test-mbi-a-xxx 1)
            (= test-mbi-b-xxx 2)
            (= test-mbi-c-xxx 3)))) "#,
        expect,
    );
}
