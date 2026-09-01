//! Complex combo batch 323 — `hook` ultimate: run-hooks/run-hook-with-args/
//! run-hook-with-args-until-success/until-failure/run-hook-wrapped,
//! add-hook with :depth, permanent-local hooks, buffer-local vs global.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx323_run_hooks_with_global_and_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (add-hook 'neo-cx323-hook (lambda () (push :global calls)))
  (let ((buf (get-buffer-create " *neo-cx323-hooks*")))
    (with-current-buffer buf
      (add-hook 'neo-cx323-hook (lambda () (push :local calls)) nil t)
      (run-hooks 'neo-cx323-hook))
    (let ((in-buf (nreverse calls)))
      (setq calls nil)
      (with-temp-buffer (run-hooks 'neo-cx323-hook))
      (let ((in-temp (nreverse calls)))
        (kill-buffer buf)
        (list in-buf in-temp))))
  (remove-hook 'neo-cx323-hook (lambda () (push :global calls))))
"##,
        expect,
    )
}

#[test]
fn div_cx323_run_hook_with_args_until_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:success (:h3 :h2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (let ((fn1 (lambda () (push :h1 calls) nil))
        (fn2 (lambda () (push :h2 calls) :success))
        (fn3 (lambda () (push :h3 calls) nil)))
    (add-hook 'neo-cx323-succ-hook fn1)
    (add-hook 'neo-cx323-succ-hook fn2)
    (add-hook 'neo-cx323-succ-hook fn3)
    (let ((result (run-hook-with-args-until-success 'neo-cx323-succ-hook)))
      (prog1 (list result (nreverse calls))
        (remove-hook 'neo-cx323-succ-hook fn1)
        (remove-hook 'neo-cx323-succ-hook fn2)
        (remove-hook 'neo-cx323-succ-hook fn3)))))
"##,
        expect,
    )
}

#[test]
fn div_cx323_run_hook_with_args_until_failure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (:h3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (let ((fn1 (lambda () (push :h1 calls) nil))
        (fn2 (lambda () (push :h2 calls) :fail))
        (fn3 (lambda () (push :h3 calls) nil)))
    (add-hook 'neo-cx323-fail-hook fn1)
    (add-hook 'neo-cx323-fail-hook fn2)
    (add-hook 'neo-cx323-fail-hook fn3)
    (let ((result (run-hook-with-args-until-failure 'neo-cx323-fail-hook)))
      (prog1 (list result (nreverse calls))
        (remove-hook 'neo-cx323-fail-hook fn1)
        (remove-hook 'neo-cx323-fail-hook fn2)
        (remove-hook 'neo-cx323-fail-hook fn3)))))
"##,
        expect,
    )
}

#[test]
fn div_cx323_run_hook_wrapped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:wrap-enter :normal :wrap-exit)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (let ((fn (lambda () (push :normal calls))))
    (add-hook 'neo-cx323-wrap-hook fn)
    (run-hook-wrapped 'neo-cx323-wrap-hook
                      (lambda (hook-fn)
                        (push :wrap-enter calls)
                        (funcall hook-fn)
                        (push :wrap-exit calls)
                        nil))
    (prog1 (nreverse calls)
      (remove-hook 'neo-cx323-wrap-hook fn))))
"##,
        expect,
    )
}

#[test]
fn div_cx323_add_hook_with_depth_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:one :two :three)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (add-hook 'neo-cx323-depth-hook (lambda () (push :one calls)) :depth 10)
  (add-hook 'neo-cx323-depth-hook (lambda () (push :two calls)) :depth -1)
  (add-hook 'neo-cx323-depth-hook (lambda () (push :three calls)) :depth 5)
  (run-hooks 'neo-cx323-depth-hook)
  (let ((result (nreverse calls)))
    (setq neo-cx323-depth-hook nil)
    result))
"##,
        expect,
    )
}

#[test]
fn div_cx323_hook_permanent_local_survives_kill_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:perm)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (put 'neo-cx323-perm-hook 'permanent-local t)
  (let ((buf (get-buffer-create " *neo-cx323-perm*")))
    (with-current-buffer buf
      (add-hook 'neo-cx323-perm-hook (lambda () (push :perm calls)) nil t))
    (with-current-buffer buf
      (kill-all-local-variables)
      (run-hooks 'neo-cx323-perm-hook))
    (let ((result (nreverse calls)))
      (kill-buffer buf)
      result)))
"##,
        expect,
    )
}

#[test]
fn div_cx323_hook_with_args_two_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:h2 :arg1 :arg2) (:h1 :arg1 :arg2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (let ((fn1 (lambda (a b) (push (list :h1 a b) calls)))
        (fn2 (lambda (a b) (push (list :h2 a b) calls))))
    (add-hook 'neo-cx323-2arg-hook fn1)
    (add-hook 'neo-cx323-2arg-hook fn2)
    (run-hook-with-args 'neo-cx323-2arg-hook :arg1 :arg2)
    (prog1 (nreverse calls)
      (remove-hook 'neo-cx323-2arg-hook fn1)
      (remove-hook 'neo-cx323-2arg-hook fn2))))
"##,
        expect,
    )
}

#[test]
fn div_cx323_hook_nil_and_empty_var_no_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((neo-cx323-empty-hook nil))
  (list (run-hooks 'neo-cx323-empty-hook)
        (run-hook-with-args-until-success 'neo-cx323-empty-hook :x)
        (run-hook-with-args-until-failure 'neo-cx323-empty-hook :x)))
"##,
        expect,
    )
}

#[test]
fn div_cx323_hook_symbol_and_function_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:via-lambda :via-symbol)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx323-sym-fn () (push :via-symbol calls))
  (add-hook 'neo-cx323-mixed-hook 'neo-cx323-sym-fn)
  (add-hook 'neo-cx323-mixed-hook (lambda () (push :via-lambda calls)))
  (run-hooks 'neo-cx323-mixed-hook)
  (prog1 (nreverse calls)
    (remove-hook 'neo-cx323-mixed-hook 'neo-cx323-sym-fn)))
"##,
        expect,
    )
}

#[test]
fn div_cx323_hook_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (add-hook 'after-change-functions
            (lambda (&rest _) (push :change calls)) nil t)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Hook mega test buffer content")
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((change-count-1 (length calls)))
        (setq calls nil)
        (delete-region 5 9)
        (insert "X")
        (let ((state (list change-count-1 (length calls)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen()
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    )
}
