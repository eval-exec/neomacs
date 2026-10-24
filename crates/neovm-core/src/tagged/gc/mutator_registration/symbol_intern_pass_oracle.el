;; -*- lexical-binding: t -*-
;; Held symbol identity and live reads across an append-only initial intern.
(unless (and (string= emacs-version "31.1")
             (equal (getenv "NEOVM_ORACLE_MODE") "refresh")
             (equal (getenv "UPDATE_EXPECT") "1"))
  (error "C5 oracle requires GNU 31.1 and explicit refresh mode"))
(setq p3sym-c5-held-pass "A")
(let* ((held (intern "p3sym-c5-held-pass"))
       (first (symbol-value held))
       (fresh (intern (copy-sequence "p3sym-c5-held-pass-fresh"))))
  (prin1 (concat first (symbol-value held))) (terpri)
  (prin1 (eq held (intern "p3sym-c5-held-pass"))) (terpri)
  (prin1 (boundp fresh)) (terpri))
