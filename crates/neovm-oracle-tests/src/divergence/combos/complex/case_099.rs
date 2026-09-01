//! Complex combo batch 99 — comint / compile / eww / shell / term / ielm
//! package availability, hook semantics, and command loop dispatch.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx99_comint_availability_and_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'comint)
      (list (fboundp 'make-comint)
            (fboundp 'comint-run)
            (boundp 'comint-prompt-read-only)
            (boundp 'comint-buffer-maximum-size)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx99_compile_buffer_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'compile)
      (list (fboundp 'compile)
            (fboundp 'recompile)
            (boundp 'compilation-error-regexp-alist)
            (fboundp 'compilation-mode)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx99_eww_browser_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eww)
      (list (fboundp 'eww)
            (fboundp 'eww-browse-url)
            (boundp 'eww-search-prefix)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx99_shell_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'shell)
      (list (fboundp 'shell)
            (fboundp 'make-shell-command)
            (boundp 'explicit-shell-file-name)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx99_ielm_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ielm)
      (list (fboundp 'ielm)
            (fboundp 'ielm-change-working-buffer)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx99_run_hooks_with_buffer_local_and_global() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (add-hook 'neo-cx99-hook (lambda () (push :global calls)))
  (let ((buf (get-buffer-create " *neo-cx99-hooks*")))
    (with-current-buffer buf
      (add-hook 'neo-cx99-hook (lambda () (push :local calls)) nil t)
      (run-hooks 'neo-cx99-hook))
    (let ((in-buf (nreverse calls)))
      (setq calls nil)
      (with-temp-buffer
        (run-hooks 'neo-cx99-hook))
      (let ((in-temp (nreverse calls)))
        (kill-buffer buf)
        (list in-buf in-temp))))
  (remove-hook 'neo-cx99-hook (lambda () (push :global calls))))
"##,
        expect,
    );
}

#[test]
fn div_cx99_run_hook_with_args_through_two_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:h2 :a :b :c) (:h1 :a :b :c))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (collected)
  (let ((fn1 (lambda (&rest args) (push (cons :h1 args) collected)))
        (fn2 (lambda (&rest args) (push (cons :h2 args) collected))))
    (add-hook 'neo-cx99-arg-hook fn1)
    (add-hook 'neo-cx99-arg-hook fn2)
    (run-hook-with-args 'neo-cx99-arg-hook :a :b :c)
    (prog1 (nreverse collected)
      (remove-hook 'neo-cx99-arg-hook fn1)
      (remove-hook 'neo-cx99-arg-hook fn2))))
"##,
        expect,
    );
}

#[test]
fn div_cx99_run_hook_with_args_until_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:success (:h3 :h2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (collected)
  (let ((fn1 (lambda () (push :h1 collected) nil))
        (fn2 (lambda () (push :h2 collected) :success))
        (fn3 (lambda () (push :h3 collected) nil)))
    (add-hook 'neo-cx99-succ-hook fn1)
    (add-hook 'neo-cx99-succ-hook fn2)
    (add-hook 'neo-cx99-succ-hook fn3)
    (let ((result (run-hook-with-args-until-success 'neo-cx99-succ-hook)))
      (prog1 (list result (nreverse collected))
        (remove-hook 'neo-cx99-succ-hook fn1)
        (remove-hook 'neo-cx99-succ-hook fn2)
        (remove-hook 'neo-cx99-succ-hook fn3)))))
"##,
        expect,
    );
}

#[test]
fn div_cx99_run_hook_with_args_until_failure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (:h3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (collected)
  (let ((fn1 (lambda () (push :h1 collected) nil))
        (fn2 (lambda () (push :h2 collected) :fail))
        (fn3 (lambda () (push :h3 collected) nil)))
    (add-hook 'neo-cx99-fail-hook fn1)
    (add-hook 'neo-cx99-fail-hook fn2)
    (add-hook 'neo-cx99-fail-hook fn3)
    (let ((result (run-hook-with-args-until-failure 'neo-cx99-fail-hook)))
      (prog1 (list result (nreverse collected))
        (remove-hook 'neo-cx99-fail-hook fn1)
        (remove-hook 'neo-cx99-fail-hook fn2)
        (remove-hook 'neo-cx99-fail-hook fn3)))))
"##,
        expect,
    );
}

#[test]
fn div_cx99_run_hook_wrapped_with_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:wrap-enter :normal :wrap-exit)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (collected)
  (let ((fn1 (lambda () (push :normal collected))))
    (add-hook 'neo-cx99-wrapped-hook fn1)
    (run-hook-wrapped 'neo-cx99-wrapped-hook
                      (lambda (hook-fn)
                        (push :wrap-enter collected)
                        (funcall hook-fn)
                        (push :wrap-exit collected)
                        nil))
    (prog1 (nreverse collected)
      (remove-hook 'neo-cx99-wrapped-hook fn1))))
"##,
        expect,
    );
}

#[test]
fn div_cx99_define_minor_mode_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-minor-mode neo-cx99-minor
        "Test minor mode."
        :init-value nil
        :global t
        (if neo-cx99-minor
            (message "on")
          (message "off")))
      (let ((before neo-cx99-minor))
        (neo-cx99-minor 1)
        (let ((after-on neo-cx99-minor))
          (neo-cx99-minor 0)
          (list (commandp 'neo-cx99-minor)
                before after-on neo-cx99-minor))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx99_hooks_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (add-hook 'after-change-functions
            (lambda (&rest _) (push :change calls)) nil t)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Hook test buffer content here")
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((change-count-1 (length calls)))
        (setq calls nil)
        (delete-region 5 9)
        (let ((change-count-2 (length calls)))
          (insert "X")
          (let ((state (list change-count-1 change-count-2
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))))
"##,
        expect,
    );
}
