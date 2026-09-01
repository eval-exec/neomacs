//! Oracle parity tests for buffer-local variables with complex patterns:
//! make-local-variable, make-variable-buffer-local, buffer-local-value,
//! local-variable-p, kill-local-variable, per-buffer settings, and
//! buffer-local vs default value interaction.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-local-variable: create a buffer-local binding in one buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_local_make_local_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // make-local-variable creates a buffer-local binding in the current buffer.
    // The variable retains the default value in other buffers.
    let form = r#"(progn
  (defvar neovm--blv-test-1 'default-val)
  (unwind-protect
      (let ((buf1 (generate-new-buffer " *neovm-blv-test-1a*"))
            (buf2 (generate-new-buffer " *neovm-blv-test-1b*")))
        (unwind-protect
            (progn
              ;; In buf1: make local and set
              (with-current-buffer buf1
                (make-local-variable 'neovm--blv-test-1)
                (setq neovm--blv-test-1 'local-val-1))
              ;; In buf2: leave as default
              (list
               ;; buf1 has local value
               (buffer-local-value 'neovm--blv-test-1 buf1)
               ;; buf2 has default value
               (buffer-local-value 'neovm--blv-test-1 buf2)
               ;; local-variable-p
               (with-current-buffer buf1
                 (local-variable-p 'neovm--blv-test-1))
               (with-current-buffer buf2
                 (local-variable-p 'neovm--blv-test-1))
               ;; Default value unchanged
               (default-value 'neovm--blv-test-1)))
          (kill-buffer buf1)
          (kill-buffer buf2)))
    (makunbound 'neovm--blv-test-1)))"#;
    let expect = expect_test::expect![r#""OK (local-val-1 default-val t nil default-val)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-variable-buffer-local: automatically buffer-local in all buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_local_make_variable_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // make-variable-buffer-local makes any setq in any buffer create a local binding.
    let form = r#"(progn
  (defvar neovm--blv-test-2 'global-default)
  (unwind-protect
      (progn
        (make-variable-buffer-local 'neovm--blv-test-2)
        (let ((buf1 (generate-new-buffer " *neovm-blv-test-2a*"))
              (buf2 (generate-new-buffer " *neovm-blv-test-2b*"))
              (buf3 (generate-new-buffer " *neovm-blv-test-2c*")))
          (unwind-protect
              (progn
                ;; Set in buf1 -- automatically becomes local
                (with-current-buffer buf1
                  (setq neovm--blv-test-2 'val-a))
                ;; Set in buf2 -- automatically becomes local
                (with-current-buffer buf2
                  (setq neovm--blv-test-2 'val-b))
                ;; buf3 untouched -- sees default
                (list
                 (buffer-local-value 'neovm--blv-test-2 buf1)
                 (buffer-local-value 'neovm--blv-test-2 buf2)
                 (buffer-local-value 'neovm--blv-test-2 buf3)
                 ;; All are local-variable-p after setq (for buf1, buf2)
                 (with-current-buffer buf1
                   (local-variable-p 'neovm--blv-test-2))
                 (with-current-buffer buf2
                   (local-variable-p 'neovm--blv-test-2))
                 ;; buf3 may or may not show local-variable-p depending on
                 ;; automatically-buffer-local status
                 (default-value 'neovm--blv-test-2)))
            (kill-buffer buf1)
            (kill-buffer buf2)
            (kill-buffer buf3))))
    (makunbound 'neovm--blv-test-2)))"#;
    let expect = expect_test::expect![r#""OK (val-a val-b global-default t t global-default)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// buffer-local-value reads from a specific buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_local_value_reads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // buffer-local-value can read the local value of any buffer without
    // switching to it.
    let form = r#"(progn
  (defvar neovm--blv-test-3 0)
  (unwind-protect
      (let ((bufs nil))
        ;; Create 5 buffers, each with a different local value
        (dotimes (i 5)
          (let ((buf (generate-new-buffer
                      (format " *neovm-blv-test-3-%d*" i))))
            (with-current-buffer buf
              (make-local-variable 'neovm--blv-test-3)
              (setq neovm--blv-test-3 (* (1+ i) 10)))
            (push buf bufs)))
        (setq bufs (nreverse bufs))
        (unwind-protect
            (list
             ;; Read each buffer's local value
             (mapcar (lambda (b) (buffer-local-value 'neovm--blv-test-3 b))
                     bufs)
             ;; Default value is still 0
             (default-value 'neovm--blv-test-3)
             ;; Modify one buffer and re-read
             (progn
               (with-current-buffer (nth 2 bufs)
                 (setq neovm--blv-test-3 999))
               (buffer-local-value 'neovm--blv-test-3 (nth 2 bufs)))
             ;; Others unchanged
             (buffer-local-value 'neovm--blv-test-3 (nth 0 bufs))
             (buffer-local-value 'neovm--blv-test-3 (nth 4 bufs)))
          (dolist (b bufs) (kill-buffer b))))
    (makunbound 'neovm--blv-test-3)))"#;
    let expect = expect_test::expect![r#""OK ((10 20 30 40 50) 0 999 10 50)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// kill-local-variable removes the local binding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_local_kill_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // kill-local-variable removes the buffer-local binding, reverting to default.
    let form = r#"(progn
  (defvar neovm--blv-test-4 'the-default)
  (unwind-protect
      (let ((buf (generate-new-buffer " *neovm-blv-test-4*")))
        (unwind-protect
            (with-current-buffer buf
              ;; Make local and set
              (make-local-variable 'neovm--blv-test-4)
              (setq neovm--blv-test-4 'local-override)
              (let ((before-kill (list
                                  neovm--blv-test-4
                                  (local-variable-p 'neovm--blv-test-4))))
                ;; Kill the local binding
                (kill-local-variable 'neovm--blv-test-4)
                (let ((after-kill (list
                                   neovm--blv-test-4
                                   (local-variable-p 'neovm--blv-test-4))))
                  ;; Re-create local binding with different value
                  (make-local-variable 'neovm--blv-test-4)
                  (setq neovm--blv-test-4 'second-local)
                  (let ((re-local (list
                                   neovm--blv-test-4
                                   (local-variable-p 'neovm--blv-test-4))))
                    ;; Kill again
                    (kill-local-variable 'neovm--blv-test-4)
                    (list
                     before-kill
                     after-kill
                     re-local
                     neovm--blv-test-4
                     (local-variable-p 'neovm--blv-test-4))))))
          (kill-buffer buf)))
    (makunbound 'neovm--blv-test-4)))"#;
    let expect = expect_test::expect![
        r#""OK ((local-override t) (the-default nil) (second-local t) the-default nil)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// local-variable-p with various types of variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_local_variable_p_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test local-variable-p for: non-existent local, explicitly local,
    // automatically buffer-local, built-in always-local variables.
    let form = r#"(progn
  (defvar neovm--blv-test-5 nil)
  (defvar neovm--blv-test-5b nil)
  (unwind-protect
      (progn
        (make-variable-buffer-local 'neovm--blv-test-5b)
        (let ((buf (generate-new-buffer " *neovm-blv-test-5*")))
          (unwind-protect
              (with-current-buffer buf
                (list
                 ;; neovm--blv-test-5: not yet local
                 (local-variable-p 'neovm--blv-test-5)
                 ;; Make it local
                 (progn (make-local-variable 'neovm--blv-test-5)
                        (local-variable-p 'neovm--blv-test-5))
                 ;; neovm--blv-test-5b: automatically buffer-local, but
                 ;; before setq it depends on implementation
                 (local-variable-p 'neovm--blv-test-5b)
                 ;; After setq
                 (progn (setq neovm--blv-test-5b 'something)
                        (local-variable-p 'neovm--blv-test-5b))
                 ;; A nonexistent variable
                 (local-variable-p 'neovm--blv-does-not-exist-91827)
                 ;; Built-in buffer-local: major-mode
                 (local-variable-p 'major-mode)))
            (kill-buffer buf))))
    (makunbound 'neovm--blv-test-5)
    (makunbound 'neovm--blv-test-5b)))"#;
    let expect = expect_test::expect![r#""OK (nil t nil t nil t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: per-buffer settings system using buffer-local variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_local_settings_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a per-buffer settings system: define settings with defaults,
    // override per-buffer, query, and reset individual settings.
    let form = r#"(progn
  ;; Registry of setting symbols
  (defvar neovm--blv-settings-registry nil)

  ;; Define a setting (creates a defvar and records it)
  (fset 'neovm--blv-define-setting
    (lambda (name default)
      (set name default)
      (make-variable-buffer-local name)
      (push name neovm--blv-settings-registry)
      name))

  ;; Get setting value in a buffer
  (fset 'neovm--blv-get-setting
    (lambda (buf name)
      (buffer-local-value name buf)))

  ;; Set setting value in a buffer
  (fset 'neovm--blv-set-setting
    (lambda (buf name value)
      (with-current-buffer buf
        (set name value))))

  ;; Reset a setting in a buffer to default
  (fset 'neovm--blv-reset-setting
    (lambda (buf name)
      (with-current-buffer buf
        (kill-local-variable name))))

  ;; Get all settings for a buffer as alist
  (fset 'neovm--blv-get-all-settings
    (lambda (buf)
      (let ((result nil))
        (dolist (name neovm--blv-settings-registry)
          (push (cons name (buffer-local-value name buf)) result))
        (nreverse result))))

  (unwind-protect
      (progn
        ;; Define three settings
        (funcall 'neovm--blv-define-setting 'neovm--blv-s-indent 4)
        (funcall 'neovm--blv-define-setting 'neovm--blv-s-tab-width 8)
        (funcall 'neovm--blv-define-setting 'neovm--blv-s-fill-col 70)
        (let ((buf1 (generate-new-buffer " *neovm-blv-settings-1*"))
              (buf2 (generate-new-buffer " *neovm-blv-settings-2*")))
          (unwind-protect
              (progn
                ;; Customize buf1
                (funcall 'neovm--blv-set-setting buf1 'neovm--blv-s-indent 2)
                (funcall 'neovm--blv-set-setting buf1 'neovm--blv-s-fill-col 80)
                ;; Customize buf2
                (funcall 'neovm--blv-set-setting buf2 'neovm--blv-s-tab-width 4)
                (list
                 ;; buf1 settings
                 (funcall 'neovm--blv-get-setting buf1 'neovm--blv-s-indent)
                 (funcall 'neovm--blv-get-setting buf1 'neovm--blv-s-tab-width)
                 (funcall 'neovm--blv-get-setting buf1 'neovm--blv-s-fill-col)
                 ;; buf2 settings
                 (funcall 'neovm--blv-get-setting buf2 'neovm--blv-s-indent)
                 (funcall 'neovm--blv-get-setting buf2 'neovm--blv-s-tab-width)
                 (funcall 'neovm--blv-get-setting buf2 'neovm--blv-s-fill-col)
                 ;; Reset indent in buf1 -> reverts to default
                 (progn
                   (funcall 'neovm--blv-reset-setting buf1 'neovm--blv-s-indent)
                   (funcall 'neovm--blv-get-setting buf1 'neovm--blv-s-indent))
                 ;; All settings for buf2
                 (funcall 'neovm--blv-get-all-settings buf2)))
            (kill-buffer buf1)
            (kill-buffer buf2))))
    (makunbound 'neovm--blv-settings-registry)
    (makunbound 'neovm--blv-s-indent)
    (makunbound 'neovm--blv-s-tab-width)
    (makunbound 'neovm--blv-s-fill-col)
    (fmakunbound 'neovm--blv-define-setting)
    (fmakunbound 'neovm--blv-get-setting)
    (fmakunbound 'neovm--blv-set-setting)
    (fmakunbound 'neovm--blv-reset-setting)
    (fmakunbound 'neovm--blv-get-all-settings)))"#;
    let expect = expect_test::expect![
        r#""OK (2 8 80 4 4 70 4 ((neovm--blv-s-fill-col . 70) (neovm--blv-s-tab-width . 4) (neovm--blv-s-indent . 4)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: buffer-local vs default value interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_local_default_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test the interaction between set-default, buffer-local bindings,
    // and how changes to the default affect buffers without local bindings.
    let form = r#"(progn
  (defvar neovm--blv-test-6 'initial)
  (unwind-protect
      (let ((buf-local (generate-new-buffer " *neovm-blv-test-6-local*"))
            (buf-default (generate-new-buffer " *neovm-blv-test-6-default*")))
        (unwind-protect
            (progn
              ;; buf-local gets a local binding
              (with-current-buffer buf-local
                (make-local-variable 'neovm--blv-test-6)
                (setq neovm--blv-test-6 'local-val))
              ;; buf-default uses the default
              (let ((phase1 (list
                             (buffer-local-value 'neovm--blv-test-6 buf-local)
                             (buffer-local-value 'neovm--blv-test-6 buf-default)
                             (default-value 'neovm--blv-test-6))))
                ;; Change the default value
                (set-default 'neovm--blv-test-6 'new-default)
                (let ((phase2 (list
                               ;; buf-local still has its override
                               (buffer-local-value 'neovm--blv-test-6 buf-local)
                               ;; buf-default sees the new default
                               (buffer-local-value 'neovm--blv-test-6 buf-default)
                               (default-value 'neovm--blv-test-6))))
                  ;; Kill the local variable in buf-local -> falls back to default
                  (with-current-buffer buf-local
                    (kill-local-variable 'neovm--blv-test-6))
                  (let ((phase3 (list
                                 (buffer-local-value 'neovm--blv-test-6 buf-local)
                                 (buffer-local-value 'neovm--blv-test-6 buf-default)
                                 (default-value 'neovm--blv-test-6))))
                    ;; Change default again -- both buffers should see it
                    (set-default 'neovm--blv-test-6 'final-default)
                    (let ((phase4 (list
                                   (buffer-local-value 'neovm--blv-test-6 buf-local)
                                   (buffer-local-value 'neovm--blv-test-6 buf-default)
                                   (default-value 'neovm--blv-test-6))))
                      (list phase1 phase2 phase3 phase4))))))
          (kill-buffer buf-local)
          (kill-buffer buf-default)))
    (makunbound 'neovm--blv-test-6)))"#;
    let expect = expect_test::expect![
        r#""OK ((local-val initial initial) (local-val new-default new-default) (new-default new-default new-default) (final-default final-default final-default))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-local variables with let-binding interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_local_let_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test how let-binding interacts with buffer-local variables.
    // A let-binding temporarily overrides the local value within the let scope.
    let form = r#"(progn
  (defvar neovm--blv-test-7 'outer-default)
  (unwind-protect
      (let ((buf (generate-new-buffer " *neovm-blv-test-7*")))
        (unwind-protect
            (with-current-buffer buf
              (make-local-variable 'neovm--blv-test-7)
              (setq neovm--blv-test-7 'buffer-val)
              (let ((before-let neovm--blv-test-7))
                ;; let-bind temporarily overrides
                (let ((neovm--blv-test-7 'let-override))
                  (let ((inside-let neovm--blv-test-7))
                    ;; After let exits, restores
                    (list
                     before-let
                     inside-let
                     ;; Nested let
                     (let ((neovm--blv-test-7 'nested))
                       neovm--blv-test-7))))
                ;; After all lets
                neovm--blv-test-7))
          (kill-buffer buf)))
    (makunbound 'neovm--blv-test-7)))"#;
    let expect = expect_test::expect![r#""OK buffer-val""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
