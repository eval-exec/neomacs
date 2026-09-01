//! Complex combo batch 261 — `hook` deep: `run-hooks` / `run-hook-with-
//! args` / `run-hook-with-args-until-success` / `run-hook-with-args-
//! until-failure` / `run-hook-wrapped` / `add-hook` `:depth` /
//! `remove-hook` / `kill-all-local-variables`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx261_run_hooks_with_nil_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :ran-no-error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((neo-cx261-hook nil))
  (run-hooks 'neo-cx261-hook)
  :ran-no-error)
"##,
        expect,
    )
}

#[test]
fn div_cx261_add_hook_with_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:first :second :third)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (let ((fn1 (lambda () (push :first calls)))
        (fn2 (lambda () (push :second calls)))
        (fn3 (lambda () (push :third calls))))
    (add-hook 'neo-cx261-depth-hook fn1 :depth 10)
    (add-hook 'neo-cx261-depth-hook fn2 :depth -1)
    (add-hook 'neo-cx261-depth-hook fn3 :depth 5)
    (run-hooks 'neo-cx261-depth-hook)
    (prog1 (nreverse calls)
      (remove-hook 'neo-cx261-depth-hook fn1)
      (remove-hook 'neo-cx261-depth-hook fn2)
      (remove-hook 'neo-cx261-depth-hook fn3))))
"##,
        expect,
    )
}

#[test]
fn div_cx261_run_hook_with_args_until_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:success ((:h3 :arg) (:h2 :arg)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (let ((fn1 (lambda (x) (push (list :h1 x) calls) nil))
        (fn2 (lambda (x) (push (list :h2 x) calls) :success))
        (fn3 (lambda (x) (push (list :h3 x) calls) nil)))
    (add-hook 'neo-cx261-succ-hook fn1)
    (add-hook 'neo-cx261-succ-hook fn2)
    (add-hook 'neo-cx261-succ-hook fn3)
    (let ((result (run-hook-with-args-until-success 'neo-cx261-succ-hook :arg)))
      (prog1 (list result (nreverse calls))
        (remove-hook 'neo-cx261-succ-hook fn1)
        (remove-hook 'neo-cx261-succ-hook fn2)
        (remove-hook 'neo-cx261-succ-hook fn3)))))
"##,
        expect,
    )
}

#[test]
fn div_cx261_run_hook_with_args_until_failure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil ((:h3 :arg)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (let ((fn1 (lambda (x) (push (list :h1 x) calls) nil))
        (fn2 (lambda (x) (push (list :h2 x) calls) :fail))
        (fn3 (lambda (x) (push (list :h3 x) calls) nil)))
    (add-hook 'neo-cx261-fail-hook fn1)
    (add-hook 'neo-cx261-fail-hook fn2)
    (add-hook 'neo-cx261-fail-hook fn3)
    (let ((result (run-hook-with-args-until-failure 'neo-cx261-fail-hook :arg)))
      (prog1 (list result (nreverse calls))
        (remove-hook 'neo-cx261-fail-hook fn1)
        (remove-hook 'neo-cx261-fail-hook fn2)
        (remove-hook 'neo-cx261-fail-hook fn3)))))
"##,
        expect,
    )
}

#[test]
fn div_cx261_run_hook_wrapped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:wrap-enter :normal :wrap-exit)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (let ((fn (lambda () (push :normal calls))))
    (add-hook 'neo-cx261-wrap-hook fn)
    (run-hook-wrapped 'neo-cx261-wrap-hook
                      (lambda (hook-fn)
                        (push :wrap-enter calls)
                        (funcall hook-fn)
                        (push :wrap-exit calls)
                        nil))
    (prog1 (nreverse calls)
      (remove-hook 'neo-cx261-wrap-hook fn))))
"##,
        expect,
    )
}

#[test]
fn div_cx261_hook_buffer_local_persistence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (let ((buf (get-buffer-create " *neo-cx261-local*")))
    (with-current-buffer buf
      (add-hook 'neo-cx261-local-hook (lambda () (push :local calls)) nil t)
      (run-hooks 'neo-cx261-local-hook))
    (let ((in-buf (nreverse calls)))
      (setq calls nil)
      (with-temp-buffer
        (run-hooks 'neo-cx261-local-hook))
      (let ((in-temp (nreverse calls)))
        (kill-buffer buf)
        (list in-buf in-temp))))
  (setq neo-cx261-local-hook nil))
"##,
        expect,
    )
}

#[test]
fn div_cx261_hook_permanent_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:perm)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (put 'neo-cx261-perm-hook 'permanent-local t)
  (let ((buf (get-buffer-create " *neo-cx261-perm*")))
    (with-current-buffer buf
      (add-hook 'neo-cx261-perm-hook (lambda () (push :perm calls)) nil t))
    (with-current-buffer buf
      (kill-all-local-variables)
      (run-hooks 'neo-cx261-perm-hook))
    (let ((result (nreverse calls)))
      (kill-buffer buf)
      result)))
"##,
        expect,
    )
}

#[test]
fn div_cx261_hook_with_symbol_and_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:via-lambda :via-symbol)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (defun neo-cx261-hook-fn () (push :via-symbol calls))
  (add-hook 'neo-cx261-mixed-hook 'neo-cx261-hook-fn)
  (add-hook 'neo-cx261-mixed-hook (lambda () (push :via-lambda calls)))
  (run-hooks 'neo-cx261-mixed-hook)
  (prog1 (nreverse calls)
    (remove-hook 'neo-cx261-mixed-hook 'neo-cx261-hook-fn)))
"##,
        expect,
    )
}

#[test]
fn div_cx261_hook_nil_and_empty_hook_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (run-hook-with-args-until-success 'neo-cx261-empty-hook :x)
      (run-hook-with-args-until-failure 'neo-cx261-empty-hook :x)
      (run-hooks 'neo-cx261-empty-hook))
"##,
        expect,
    )
}

#[test]
fn div_cx261_hook_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
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
      (let ((change-count (length calls)))
        (setq calls nil)
        (delete-region 5 9)
        (insert "X")
        (let ((state (list change-count (length calls)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    )
}
