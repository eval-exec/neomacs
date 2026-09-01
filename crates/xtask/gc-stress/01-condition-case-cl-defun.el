;;; 01-condition-case-cl-defun.el --- in-flight signal payload -*- lexical-binding: t -*-
;;; expect: (error "Malformed argument list ends with: (&rest f &aux (g (+ a b)))")

;; DIVERGENCES.md 161's reduction, verbatim in shape.
;;
;; `cl-defun' with a malformed arglist signals while a buffer's
;; `unwind-protect' cleanup is pending. The signal then travels up the RUST
;; stack as `Flow::Signal', and every frame it passes runs `unbind_to' -- here
;; literally `(kill-buffer buf)'. That allocates, which is an allocation-bearing
;; safe point, which under NEOVM_GC_STRESS=1 is a full collection. Before 161
;; the payload had no root, so `condition-case' bound a cons the collector had
;; already reclaimed and the failure surfaced as an "invalid symbol id" panic in
;; the printer -- a free-list link decoding through TAG_SYMBOL.

(require 'cl-lib)

(condition-case err
    (let ((buf (generate-new-buffer " *probe*")))
      (unwind-protect
          (eval (read "(cl-defun probe-keyfn (a b &optional c &key d (e 10) &rest f &aux (g (+ a b))) (list a b c d e f g))") t)
        (when (buffer-live-p buf) (kill-buffer buf))))
  (error (prin1 (cons (car err) (cdr err))) (terpri)))
