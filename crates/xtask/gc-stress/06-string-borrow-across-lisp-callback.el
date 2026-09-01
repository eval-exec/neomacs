;;; 06-string-borrow-across-lisp-callback.el --- borrowed string bytes -*- lexical-binding: t -*-
;;; expect: (("foobar" "foobaz") "fooba" t "xxAxx" "needle")

;; DIVERGENCES.md 163.  `Value::as_lisp_string' hands out `&'static LispString'
;; -- a borrow into a GC-managed, mark-sweep heap, with a lifetime the borrow
;; checker cannot check.  A swept string's `LispString::drop' frees the byte
;; buffer (`release_owned_storage', heap_types.rs), so a borrow that outlives
;; its object reads freed memory.
;;
;; Each form below routes through a Rust builtin that takes such a borrow and
;; THEN calls back into Lisp -- a predicate, a replacement function -- which is
;; an allocation-bearing safe point.  Under NEOVM_GC_STRESS=1 every one of
;; those safe points collects, so a borrow whose object is not reachable from
;; the root set at that moment comes back as freed bytes.
;;
;; The strings are built with `concat' rather than written as literals so they
;; are FRESH heap objects rather than constants the reader's object table
;; already keeps alive.

(defvar gc-stress-sink nil)

(defun gc-stress-churn (&rest _)
  "Allocate enough to make the collection that follows non-trivial."
  (setq gc-stress-sink (make-list 256 'churn))
  t)

(let* ((prefix (concat "foo" "b"))
       (coll (list (concat "foo" "bar") (concat "foo" "baz"))))
  (prin1
   (list
    ;; `all-completions': the Rust side borrows STRING's bytes for the whole
    ;; candidate scan and calls PREDICATE for each candidate.
    (all-completions prefix coll #'gc-stress-churn)
    ;; `try-completion': same borrow, and the answer is derived from those
    ;; bytes, so corruption shows up in the value rather than only in a crash.
    (try-completion prefix coll #'gc-stress-churn)
    ;; `test-completion'.
    (test-completion (concat "foo" "bar") coll #'gc-stress-churn)
    ;; `replace-regexp-in-string' with a FUNCTION replacement: the Rust side
    ;; holds the subject and the replacement across the funcall.
    (replace-regexp-in-string "y" (lambda (_m) (gc-stress-churn) "A")
                              (concat "xx" "y" "xx"))
    ;; `match-string' reads back through the searched string AFTER the search
    ;; has returned, i.e. across at least one more safe point.
    (with-temp-buffer
      (insert (concat "hay" "needle" "stack"))
      (goto-char (point-min))
      (re-search-forward (concat "need" "le"))
      (gc-stress-churn)
      (match-string 0)))))
(terpri)
