;;; 07-string-borrow-across-change-hooks.el --- borrowed bytes vs. hooks -*- lexical-binding: t -*-
;;; expect: ("abcdefgh" "Zther" "onetwo" "princ(sym)" (#("<arg>" 0 5 (face bold)) bold))

;; DIVERGENCES.md 163.  The `&'static LispString' seam has a second failure
;; mode that rooting does not cover: a live borrow whose BYTES are relocated.
;; `LispString::mutate_bytes' (heap_types.rs) rebuilds the payload `Vec' and
;; writes back a possibly-reallocated `data' pointer, so `aset' on a string can
;; invalidate an outstanding `&LispString' with no collection involved at all.
;;
;; GNU has the same hazard and is explicit about it: `compact_small_strings'
;; (src/alloc.c) RELOCATES small string data during every GC, which is why
;; `pin_string' exists at all.  A `char *' into string data held across a GC is
;; invalid in GNU by construction.
;;
;; The insert path is where the two meet.  `insert_lisp_string_with_change_
;; hooks_in_buffer' (editfns.rs) and `insert_print_lisp_string_with_hooks'
;; (builtins/misc_eval.rs) both take `text: &LispString', run
;; `signal_before_text_change' -- which calls `before-change-functions', i.e.
;; arbitrary Lisp and therefore a safe point -- and only THEN read `text'.
;;
;; Form 1 runs `aset' from inside `before-change-functions', so a string is
;; RELOCATED (`LispString::mutate_bytes') in the middle of an insert, while the
;; insert path holds a `&LispString'.  It deliberately mutates a DIFFERENT
;; string from the one being inserted, and that stays deliberate: this probe is
;; about a borrow surviving a relocation elsewhere on the heap, which is a
;; separate hazard from which snapshot the insert reads.
;;
;; 2026-08-20: when this was written, mutating the INSERTED string was a real
;; divergence -- `insert' snapshotted its argument before the hook and GNU
;; reads it after (DIVERGENCES.md 163 §10) -- so pinning it here would have
;; failed for a reason this probe is not testing.  DIVERGENCES.md 164 landed
;; that fix; the GNU answer is now pinned by probe 09, which mutates the
;; string being inserted while consing hard.

(defvar gc-stress-sink nil)
(defvar gc-stress-other nil)

(defun gc-stress-churn (&rest _)
  (setq gc-stress-sink (make-list 256 'churn))
  nil)

(prin1
 (list
  ;; 1. a before-change function conses hard and relocates another string
  ;; while the insert path holds a borrow into the heap
  (let ((s (copy-sequence "abcdefgh")))
    (setq gc-stress-other (copy-sequence "other"))
    (with-temp-buffer
      (add-hook 'before-change-functions
                (lambda (&rest _) (gc-stress-churn) (aset gc-stress-other 0 ?Z))
                nil t)
      (insert s)
      (buffer-string)))
  gc-stress-other
  ;; 2. an after-change function that conses, with a fresh source string
  (with-temp-buffer
    (add-hook 'after-change-functions (lambda (&rest _) (gc-stress-churn)) nil t)
    (insert (concat "one" "two"))
    (buffer-string))
  ;; 3. the print sinks, whose buffer has change hooks
  (with-temp-buffer
    (add-hook 'before-change-functions (lambda (&rest _) (gc-stress-churn)) nil t)
    (princ (concat "pri" "nc") (current-buffer))
    (prin1 (list (intern (concat "sy" "m"))) (current-buffer))
    (buffer-string))
  ;; 4. `format' carries the FORMAT string's text properties onto the result,
  ;; which `apply_format_prop_spans' (builtins/strings.rs) does through a
  ;; borrow of the freshly built result string.
  (let ((f (propertize (concat "<%s>") 'face 'bold)))
    (gc-stress-churn)
    (let ((r (format f (concat "ar" "g"))))
      (list r (get-text-property 0 'face r))))))
(terpri)
