//! GNU signal-time dynamic extent and reserved evaluator depth.
//! Expectations are populated by GNU 31.1 with UPDATE_EXPECT=1.

fn check(form: &str, expected: expect_test::Expect) {
    crate::common::assert_oracle_parity_under_envs_expect(
        form,
        &[
            &[("NEOVM_JIT", "0"), ("NEOVM_TIER_I", "off")],
            &[("NEOVM_JIT", "1")],
            &[
                ("NEOVM_JIT", "1"),
                ("NEOVM_JIT_THRESHOLD", "1"),
                ("NEOVM_AOT", "0"),
                ("NEOVM_JIT_BG", "sync"),
            ],
        ],
        expected,
    );
}

#[test]
fn depth_limit_signal_hook_observes_signalling_compiled_frame() {
    check(
        r#"(progn
  (defvar gd-b-depth-frame nil)
  (defalias 'gd-b-depth-capture
    (lambda (_kind _data)
      (mapbacktrace
       (lambda (evald f args _flags)
         (when (and (null gd-b-depth-frame)
                    (memq f '(gd-b-depth-a gd-b-depth-b gd-b-depth-c)))
           (setq gd-b-depth-frame (list evald f args)))))))
  (defalias 'gd-b-depth-a
    (byte-compile (lambda (n) (if (= n 0) 0 (1+ (gd-b-depth-b (1- n)))))))
  (defalias 'gd-b-depth-b
    (byte-compile (lambda (n) (if (= n 0) 0 (1+ (gd-b-depth-c (1- n)))))))
  (defalias 'gd-b-depth-c
    (byte-compile (lambda (n) (if (= n 0) 0 (1+ (gd-b-depth-a (1- n)))))))
  (dotimes (_ 1000) (gd-b-depth-a 3))
  (let ((max-lisp-eval-depth 120)
        (signal-hook-function #'gd-b-depth-capture))
    (list (condition-case err (gd-b-depth-a 300) (error err))
          gd-b-depth-frame max-lisp-eval-depth (gd-b-depth-a 3))))"#,
        expect_test::expect![[
            r#""OK ((error \"Lisp nesting exceeds ‘max-lisp-eval-depth’\") (t gd-b-depth-b (200)) 120 3)""#
        ]],
    );
}

#[test]
fn signal_hook_at_depth_limit_uses_and_restores_reserve() {
    check(
        r#"(progn
  (defvar gd-b-depth-seen nil)
  (defalias 'gd-b-depth
    (byte-compile (lambda (n) (if (= n 0) 0 (gd-b-depth (1- n))))))
  (let ((max-lisp-eval-depth 120)
        (lisp-eval-depth-reserve 200)
        (signal-hook-function
         (lambda (_kind _data)
           (setq gd-b-depth-seen
                 (list max-lisp-eval-depth lisp-eval-depth-reserve)))))
    (list (condition-case err (gd-b-depth 300) (error (car err)))
          gd-b-depth-seen max-lisp-eval-depth lisp-eval-depth-reserve
          (gd-b-depth 3))))"#,
        expect_test::expect![[r#""OK (error (141 179) 120 200 0)""#]],
    );
}

#[test]
fn nested_signal_hook_replacement_restores_evaluator_reserve() {
    check(
        r#"(progn
  (defvar gd-b-depth-seen nil)
  (defvar gd-b-depth-count 0)
  (defalias 'gd-b-depth
    (byte-compile (lambda (n) (if (= n 0) 0 (gd-b-depth (1- n))))))
  (let ((max-lisp-eval-depth 120)
        (lisp-eval-depth-reserve 200)
        (signal-hook-function
         (lambda (kind _data)
           (push (list kind max-lisp-eval-depth lisp-eval-depth-reserve)
                 gd-b-depth-seen)
           (when (= (setq gd-b-depth-count (1+ gd-b-depth-count)) 1)
             (signal 'file-error '("from hook"))))))
    (list (condition-case err (gd-b-depth 300) (error err))
          (nreverse gd-b-depth-seen)
          max-lisp-eval-depth lisp-eval-depth-reserve)))"#,
        expect_test::expect![[
            r#""OK ((file-error \"from hook\") ((error 141 179) (file-error 146 174)) 120 200)""#
        ]],
    );
}

#[test]
fn signal_hook_throw_restores_evaluator_reserve() {
    check(
        r#"(progn
  (defalias 'gd-b-depth
    (byte-compile (lambda (n) (if (= n 0) 0 (gd-b-depth (1- n))))))
  (let ((max-lisp-eval-depth 120)
        (lisp-eval-depth-reserve 200)
        (signal-hook-function
         (lambda (_kind _data) (throw 'gd-b-hook-exit 'escaped))))
    (list (catch 'gd-b-hook-exit (gd-b-depth 300))
          max-lisp-eval-depth lisp-eval-depth-reserve)))"#,
        expect_test::expect![[r#""OK (escaped 120 200)""#]],
    );
}

#[test]
fn signal_hook_limit_change_returns_current_excess_to_reserve() {
    check(
        r#"(progn
  (defvar gd-b-depth-seen nil)
  (defalias 'gd-b-depth
    (byte-compile (lambda (n) (if (= n 0) 0 (gd-b-depth (1- n))))))
  (let ((max-lisp-eval-depth 120)
        (lisp-eval-depth-reserve 200)
        (signal-hook-function
         (lambda (_kind _data)
           (setq gd-b-depth-seen
                 (list max-lisp-eval-depth lisp-eval-depth-reserve))
           (setq max-lisp-eval-depth (+ max-lisp-eval-depth 7)))))
    (list (condition-case err (gd-b-depth 300) (error (car err)))
          gd-b-depth-seen max-lisp-eval-depth lisp-eval-depth-reserve)))"#,
        expect_test::expect![[r#""OK (error (141 179) 120 207)""#]],
    );
}

#[test]
fn localized_signal_depth_reserve_updates_the_current_binding_without_watchers() {
    check(
        r#"(progn
  (defvar gd-b-local-seen nil)
  (defvar gd-b-reserve-writes nil)
  (defalias 'gd-b-local-depth
    (byte-compile (lambda (n) (if (= n 0) 0 (gd-b-local-depth (1- n))))))
  (with-temp-buffer
    (set (make-local-variable 'max-lisp-eval-depth) 120)
    (set (make-local-variable 'lisp-eval-depth-reserve) 200)
    (dolist (variable '(max-lisp-eval-depth lisp-eval-depth-reserve))
      (add-variable-watcher
       variable
       (lambda (_symbol _value operation _where)
         (push operation gd-b-reserve-writes))))
    (let ((signal-hook-function
           (lambda (_kind _data)
             (setq gd-b-local-seen
                   (list max-lisp-eval-depth lisp-eval-depth-reserve)))))
      (list (condition-case err (gd-b-local-depth 300) (error (car err)))
            gd-b-local-seen max-lisp-eval-depth lisp-eval-depth-reserve
            gd-b-reserve-writes))))"#,
        expect_test::expect![[r#""OK (error (141 179) 120 200 nil)""#]],
    );
}

#[test]
fn localized_signal_depth_restore_uses_the_buffer_selected_by_the_hook() {
    // Prepare the signalling buffer's limit last, isolating reserve restoration
    // from the separate baseline divergence where buffer switches leave the
    // compiled-call depth cache stale.
    check(
        r#"(progn
  (defvar gd-b-local-seen nil)
  (defvar gd-b-reserve-target nil)
  (defalias 'gd-b-local-depth
    (byte-compile (lambda (n) (if (= n 0) 0 (gd-b-local-depth (1- n))))))
  (save-current-buffer
    (let ((source (get-buffer-create " *gd-b-reserve-source*"))
          (target (get-buffer-create " *gd-b-reserve-target*")))
      (unwind-protect
          (progn
            (set-buffer source)
            (set (make-local-variable 'max-lisp-eval-depth) 120)
            (set (make-local-variable 'lisp-eval-depth-reserve) 200)
            (set-buffer target)
            (set (make-local-variable 'max-lisp-eval-depth) 150)
            (set (make-local-variable 'lisp-eval-depth-reserve) 40)
            (setq gd-b-reserve-target target)
            (set-buffer source)
            (setq max-lisp-eval-depth 120)
            (let ((signal-hook-function
                   (lambda (_kind _data)
                     (set-buffer gd-b-reserve-target)
                     (setq gd-b-local-seen
                           (list max-lisp-eval-depth lisp-eval-depth-reserve))
                     (setq max-lisp-eval-depth (+ max-lisp-eval-depth 7)
                           lisp-eval-depth-reserve (+ lisp-eval-depth-reserve 3)))))
              (list (condition-case err (gd-b-local-depth 300) (error (car err)))
                    gd-b-local-seen (eq (current-buffer) target)
                    max-lisp-eval-depth lisp-eval-depth-reserve
                    (buffer-local-value 'max-lisp-eval-depth source)
                    (buffer-local-value 'lisp-eval-depth-reserve source))))
        (kill-buffer source)
        (kill-buffer target)))))"#,
        expect_test::expect![[r#""OK (error (150 40) t 120 80 141 179)""#]],
    );
}
