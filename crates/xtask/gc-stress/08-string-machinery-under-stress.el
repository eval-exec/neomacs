;;; 08-string-machinery-under-stress.el --- the whole seam, hard -*- lexical-binding: t -*-
;;; expect: (:count 96 :sample ("thE qUIck brOwn fOx" "thE qUIck brOwn fOx"))

;; DIVERGENCES.md 163.  Probes 06 and 07 aim at specific borrow sites; this one
;; is breadth.  Every form routes through `Value::as_lisp_string', and the
;; dead-string tripwire the entry added (`LispString::is_reclaimed', GNU's
;; `sweep_strings' null-data marker) is armed the whole way, so a stale borrow
;; aborts at the scene instead of returning freed bytes.
;;
;; Form 1 is the one that earns its place.  `looking-at', `re-search-forward'
;; and `search-forward' all reach `prepare_current_buffer_regexp_syntax_to',
;; which ends in `maybe_syntax_propertize_for_scan' -- an `eval.apply' of
;; `internal--syntax-propertize', i.e. arbitrary Lisp and therefore a GC safe
;; point.  Before 163 those three held a `&LispString' borrow of `args[0]'
;; across it; now the borrow lives and dies inside the block that computes the
;; syntax dependency, and this exercises the restructured path with a
;; `syntax-propertize-function' that really does allocate.
;;
;; The expected value was produced by GNU Emacs 31.0.90 and reproduces exactly.

(require 'cl-lib)

(defvar sink nil)
(defun churn (&rest _) (setq sink (make-list 128 'x)) t)

(let ((results nil))
  ;; 1. regexp search with a syntax-propertize-function installed: the four
  ;; sites DIVERGENCES.md 163 §6 restructured.
  (with-temp-buffer
    (setq-local parse-sexp-lookup-properties t)
    (setq-local syntax-propertize-function
                (lambda (start end) (churn) (ignore start end)))
    (insert "alpha beta gamma delta epsilon\n")
    (goto-char (point-min))
    (push (looking-at (concat "al" "pha")) results)
    (goto-char (point-min))
    (push (and (re-search-forward (concat "ga" "mma") nil t) t) results)
    (goto-char (point-min))
    (push (and (search-forward (concat "del" "ta") nil t) t) results))
  ;; 2. format carrying text properties, repeatedly
  (dotimes (i 20)
    (let ((f (propertize (concat "<%s:" (number-to-string i) ">") 'face 'bold)))
      (push (get-text-property 0 'face (format f (concat "a" "rg"))) results)))
  ;; 3. completion with an allocating predicate
  (let ((coll (mapcar (lambda (i) (format "cand-%d" i)) (number-sequence 0 40))))
    (push (length (all-completions "cand-1" coll #'churn)) results)
    (push (try-completion "cand-1" coll #'churn) results))
  ;; 4. coding conversion round trips
  (dotimes (_ 20)
    (push (decode-coding-string
           (encode-coding-string (concat "caf" "é") 'utf-8) 'utf-8)
          results))
  ;; 5. interning fresh names
  (dotimes (i 40)
    (push (symbol-name (intern (concat "neovm-stress-" (number-to-string i))))
          results))
  ;; 6. insertion with change hooks
  (with-temp-buffer
    (add-hook 'before-change-functions #'churn nil t)
    (add-hook 'after-change-functions #'churn nil t)
    (dotimes (i 20) (insert (format "line %d\n" i)))
    (push (buffer-size) results))
  ;; 7. replace-regexp-in-string with a function replacement
  (dotimes (_ 10)
    (push (replace-regexp-in-string
           "[aeiou]" (lambda (m) (churn) (upcase m)) (concat "the quick brown fox"))
          results))
  (prin1 (list :count (length results)
               :sample (list (nth 0 results) (nth 1 results))))
  (terpri))
