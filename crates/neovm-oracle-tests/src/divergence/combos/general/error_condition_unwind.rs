//! Divergence tests: error + condition-case + unwind-protect + signal deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_nested_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((caught args-out-of-range \"re-signaled\") (void void-variable))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list
   (condition-case e
       (condition-case inner
           (signal 'wrong-type-argument (list "inner" 42))
         (wrong-type-argument
          (signal 'args-out-of-range (list "re-signaled" (cadr inner)))))
     (args-out-of-range
      (list 'caught (car e) (cadr e))))
   (condition-case e
       (signal 'void-variable (list 'nonexistent-sym-xxx))
     (void-variable
      (list 'void (car e)))
     (error
      (list 'err (car e)))))) "#,
        expect,
    );
}

#[test]
fn divergence_unwind_protify_cleanup_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-up-log-xxx nil)
  (unwind-protect
      (unwind-protect
          (progn
            (push 'inner-body test-up-log-xxx)
            (signal 'error nil))
        (push 'inner-cleanup test-up-log-xxx))
    (push 'outer-cleanup test-up-log-xxx))
  (let ((result (nreverse test-up-log-xxx)))
    (list result
          (equal result '(inner-body inner-cleanup outer-cleanup))))) "#,
        expect,
    );
}

#[test]
fn divergence_error_propagates_through_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((caught \"from closure\") (caught-apply \"from closure\") (caught-map \"from closure\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((fn (lambda ()
              (signal 'wrong-type-argument (list "from closure")))))
    (list (condition-case e
              (funcall fn)
            (wrong-type-argument
             (list 'caught (cadr e))))
          (condition-case e
              (apply fn nil)
            (wrong-type-argument
             (list 'caught-apply (cadr e))))
          (condition-case e
              (mapcar (lambda (_) (funcall fn)) '(1))
            (wrong-type-argument
             (list 'caught-map (cadr e))))))) "#,
        expect,
    );
}

#[test]
fn divergence_unwind_protect_insert_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"START\" t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "START")
  (let ((cleanup-done nil))
    (condition-case nil
        (unwind-protect
            (progn
              (goto-char 3)
              (insert "ERROR")
              (error "boom"))
          (setq cleanup-done t)
          (delete-region 3 8))
      (error nil))
    (list (buffer-string)
          (string= (buffer-string) "START")
          cleanup-done
          (= (buffer-size) 5)))) "#,
        expect,
    );
}

#[test]
fn divergence_signal_user_data_preserved() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid error symbol\" test-sig-xxx)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (condition-case e
      (signal 'test-sig-xxx (list '(1 2 3) "hello" 42))
    (test-sig-xxx
     (list (car e)
           (cadr e)
           (equal (cadr e) '(1 2 3))
           (caddr e)
           (string= (caddr e) "hello")
           (cadddr e)
           (= (cadddr e) 42))))) "#,
        expect,
    );
}

#[test]
fn divergence_condition_case_no_error_val() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 30 t 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list
   (condition-case e
       (+ 1 2)
     (error 'caught))
   (condition-case e
       (progn (setq e 'modified) (+ 10 20))
     (error 'caught))
   (= (condition-case e (+ 1 2) (error -1)) 3)
   (condition-case e
       (let ((x 5)) (+ x x))
     (error 'caught)))) "#,
        expect,
    );
}

#[test]
fn divergence_defining_condition_handlers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 t div-error t 0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-safe-div-xxx (a b)
    (condition-case e
        (/ a b)
      (arith-error 'div-error)))
  (list (test-safe-div-xxx 10 2)
        (= (test-safe-div-xxx 10 2) 5)
        (test-safe-div-xxx 10 0)
        (eq (test-safe-div-xxx 10 0) 'div-error)
        (test-safe-div-xxx 0 5)
        (= (test-safe-div-xxx 0 5) 0))) "#,
        expect,
    );
}

#[test]
fn divergence_unwind_after_buffer_modification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA")
  (let ((saved nil))
    (unwind-protect
        (progn
          (erase-buffer)
          (insert "BBBB")
          (push (buffer-string) saved)
          (error "test"))
      (push (buffer-string) saved)
      (erase-buffer)
      (insert "CCCC"))
    (push (buffer-string) saved)
    (list (nreverse saved)
          (string= (buffer-string) "CCCC")
          (equal (nreverse saved) '("BBBB" "BBBB" "CCCC"))))) "#,
        expect,
    );
}

#[test]
fn divergence_error_while_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (error \"narrowed error\" t t \"-BBBB-CCCC\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD")
  (narrow-to-region 5 15)
  (condition-case e
      (progn
        (goto-char 1)
        (error "narrowed error"))
    (error
     (list (car e) (cadr e)
           (= (point-min) 5)
           (= (point-max) 15)
           (buffer-string))))) "#,
        expect,
    );
}

#[test]
fn divergence_debugger_ignored_with_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((before (handled \"test\")) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-dbg-log-xxx nil)
  (let ((debug-on-error t))
    (condition-case e
        (progn
          (push 'before test-dbg-log-xxx)
          (signal 'wrong-type-argument (list "test"))
          (push 'after test-dbg-log-xxx))
      (wrong-type-argument
       (push (list 'handled (cadr e)) test-dbg-log-xxx))))
  (list (nreverse test-dbg-log-xxx)
        (equal (nreverse test-dbg-log-xxx)
               '(before (handled "test"))))) "#,
        expect,
    );
}
