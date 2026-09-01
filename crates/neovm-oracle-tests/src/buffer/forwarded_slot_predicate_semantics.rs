//! Oracle guards for GNU `DEFVAR_PER_BUFFER` write predicates.
//!
//! GNU stores the predicate in the buffer-object forwarder and checks it when
//! Lisp writes a live per-buffer slot.  This is deliberately not equivalent to
//! validating every assignment to the variable: `set-default` updates the
//! shared default without calling `store_symval_forwarding`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_forwarded_buffer_slots_enforce_typed_write_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defun neomacs-oracle--capture-slot-write (thunk)
    (condition-case err
        (list 'ok (funcall thunk))
      (error (cons (car err) (cdr err)))))
  (with-temp-buffer
    (let ((old-fill-default (default-value 'fill-column))
          (old-major-default (default-value 'major-mode)))
      (unwind-protect
          (list
           ;; Always-local symbol slots validate dynamic binding, interpreted
           ;; assignment, generic `set', and byte-compiled binding.
           (neomacs-oracle--capture-slot-write
            (lambda () (let ((major-mode "not-a-mode")) major-mode)))
           (neomacs-oracle--capture-slot-write
            (lambda () (setq major-mode "not-a-mode")))
           (neomacs-oracle--capture-slot-write
            (lambda () (set 'major-mode "not-a-mode")))
           (neomacs-oracle--capture-slot-write
            (lambda ()
              (funcall
               (byte-compile
                '(lambda ()
                   (let ((major-mode "not-a-mode"))
                     ;; Keep the byte compiler from folding the entire binding
                     ;; into a constant-return function.  GNU's Bvarbind then
                     ;; reaches the same typed forwarder storage path.
                     (symbol-value 'major-mode)))))))

           ;; Conditional slots validate when assignment creates a local slot.
           (neomacs-oracle--capture-slot-write
            (lambda () (setq fill-column "wide")))
           (neomacs-oracle--capture-slot-write
            (lambda () (setq overwrite-mode 'replace-everything)))
           (neomacs-oracle--capture-slot-write
            (lambda () (setq vertical-scroll-bar 'middle)))
           (neomacs-oracle--capture-slot-write
            (lambda () (setq scroll-up-aggressively "far")))
           (neomacs-oracle--capture-slot-write
            (lambda () (setq scroll-up-aggressively 2.0)))

           ;; Nil bypasses every forwarder predicate.  Qnil predicates impose
           ;; no type restriction and preserve arbitrary non-nil Lisp values.
           (neomacs-oracle--capture-slot-write
            (lambda () (let ((major-mode nil)) major-mode)))
           (neomacs-oracle--capture-slot-write
            (lambda () (let ((buffer-read-only 'read-mostly)) buffer-read-only)))

           ;; GNU's `set-default-internal' writes buffer_defaults directly and
           ;; intentionally does not call the live-slot predicate checker.
           (neomacs-oracle--capture-slot-write
            (lambda ()
              (set-default 'fill-column "default-wide")
              (default-value 'fill-column)))
           (neomacs-oracle--capture-slot-write
            (lambda ()
              (set-default 'major-mode "default-mode")
              (default-value 'major-mode))))
        (set-default 'fill-column old-fill-default)
        (set-default 'major-mode old-major-default)))))
"#;

    assert_oracle_parity(form);
}
