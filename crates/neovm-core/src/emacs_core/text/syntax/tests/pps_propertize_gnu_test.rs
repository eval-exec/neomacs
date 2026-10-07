//! The GNU oracle forms of `neovm-oracle-tests` `syntax/pps_propertize.rs`,
//! run in process against the answers GNU 31.1 gave there
//! (`NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`): `parse-partial-sexp` runs
//! `syntax-propertize` where GNU's scan does (U0.7, P3.4 C0d).
use crate::test_utils::{oracle_expect_transcript, runtime_startup_eval_one};

/// An emacs-lisp-mode buffer: a scan from BEGV propertizes chunk by chunk
/// (15 calls to 30000), every option stops where GNU stops, and FROM itself is
/// propertized even when FROM = TO.
#[test]
fn parse_partial_sexp_propertizes_elisp_in_chunks() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(let (log out)
  (with-temp-buffer
    (dotimes (i 600)
      (insert (format "(defun f%d (x) \"doc %d ;; not a comment\" ; c %d\n  (list x '(a . b) ?\\( \"s\\\"t\" #'car))\n" i i i)))
    (emacs-lisp-mode)
    (setq-local syntax-propertize-function
                (let ((orig syntax-propertize-function))
                  (lambda (s e) (push (list s e) log) (funcall orig s e))))
    (dolist (c '((1 30000 nil nil nil)
                 (5000 12000 nil nil nil)
                 (1 1 nil nil nil)
                 (2500 2500 nil nil nil)
                 (100 9000 1 nil nil)
                 (100 9000 nil t nil)
                 (100 9000 nil nil t)
                 (100 9000 nil nil syntax-table)
                 (3000 20000 -1 nil nil)))
      (setq log nil)
      (syntax-ppss-flush-cache (point-min))
      (setq syntax-propertize--done (min (nth 0 c) 4000))
      (remove-text-properties (point-min) (point-max) '(syntax-table nil))
      (let ((r (parse-partial-sexp (nth 0 c) (nth 1 c) (nth 2 c) (nth 3 c) nil (nth 4 c))))
        (push (list c (point) syntax-propertize--done (reverse log) r) out))))
  (nreverse out))
"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(
            r#""OK (((1 30000 nil nil nil) 30000 30605 ((1 2035) (2035 4061) (4061 6077) (6077 8103) (8103 10135) (10135 12182) (12182 14229) (14229 16276) (16276 18323) (18323 20370) (20370 22417) (22417 24464) (24464 26511) (26511 28558) (28558 30605)) (2 29984 29993 nil nil nil 0 nil nil (29931 29984) nil)) ((5000 12000 nil nil nil) 12000 13161 ((3975 7023) (7023 9067) (9067 11114) (11114 13161)) (0 11953 11969 nil t nil -1 nil 11996 (11953) nil)) ((1 1 nil nil nil) 1 2035 ((1 2035)) (0 nil nil nil nil nil 0 nil nil nil nil)) ((2500 2500 nil nil nil) 2500 4529 ((2465 4529)) (0 nil nil nil nil nil 0 nil nil nil nil)) ((100 9000 1 nil nil) 132 2121 ((84 2121)) (1 131 nil nil nil nil 0 nil nil (131) nil)) ((100 9000 nil t nil) 100 2121 ((84 2121)) (0 nil nil nil nil nil 0 nil nil nil nil)) ((100 9000 nil nil t) 106 2121 ((84 2121)) (0 nil 103 nil t nil 0 nil 105 nil nil)) ((100 9000 nil nil syntax-table) 106 2121 ((84 2121)) (0 nil 103 nil t nil 0 nil 105 nil nil)) ((3000 20000 -1 nil nil) 3066 5007 ((2981 5007)) (-1 nil 3031 nil nil nil -1 nil nil nil nil)))""#
        )
    );
}

/// Every FROM/TO/`done` combination around the trigger positions, under C,
/// nested and Lisp comment dialects, with every option: the call log of each
/// of the ~18,000 scans, digested, plus the calls at TO for FROM = 1.
#[test]
fn parse_partial_sexp_propertize_positions_sweep() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(let (log all readable)
  (dolist (dialect '(c nested lisp))
    (with-temp-buffer
      (let ((st (make-syntax-table)))
        (pcase dialect
          ('c (modify-syntax-entry ?/ ". 124b" st)
              (modify-syntax-entry ?* ". 23" st)
              (modify-syntax-entry ?\n "> b" st))
          ('nested (modify-syntax-entry ?\( "()1n" st)
                   (modify-syntax-entry ?\) ")(4n" st)
                   (modify-syntax-entry ?* ". 23n" st))
          ('lisp (modify-syntax-entry ?\; "<" st)
                 (modify-syntax-entry ?\n ">" st)
                 (setq-local comment-end-can-be-escaped t)))
        (modify-syntax-entry ?\" "\"" st)
        (modify-syntax-entry ?\\ "\\" st)
        (set-syntax-table st))
      (insert (pcase dialect
                ('c ". . ab /* xy */ (cd \"e\\\"f\" g) // hi\nzz (q) \\r")
                ('nested ". (* a (* b *) c *) (d \"x\\\"y\" e) (**) \\q")
                ('lisp ". ab ; c \\\n d\n (e \"f\\\"g\" h) ; i\n\\j")))
      (setq-local parse-sexp-lookup-properties t)
      (setq-local syntax-propertize-function
                  (lambda (s e) (push (list s e) log)))
      (let ((pmax (point-max)))
        (let ((from 1))
          (while (<= from pmax)
            (let ((to from))
              (while (<= to pmax)
                (dolist (done (delete-dups (list (1- from) from (1+ from) (1- to) to (1+ to) (/ (+ from to) 2))))
                  (dolist (opts '((nil nil nil) (0 nil nil) (1 nil nil) (nil t nil) (nil nil t) (nil nil syntax-table)))
                    (setq log nil)
                    (setq syntax-propertize--done done)
                    (goto-char pmax)
                    (let* ((r (condition-case err
                                  (parse-partial-sexp from to (nth 0 opts) (nth 1 opts) nil (nth 2 opts))
                                (error (list 'ERR err))))
                           ;; Element 10 after a STOPBEFORE stop is a separate,
                           ;; pinned divergence (parse_state.rs).
                           (r (if (and (nth 1 opts) (not (eq (car r) 'ERR))) (butlast r) r))
                           (row (list dialect from to done opts (point) syntax-propertize--done (reverse log) r)))
                      (push row all)
                      (when (and (= from 1) (= done to) (equal opts '(nil nil nil)))
                        (push (list to (length log)) readable)))))
                (setq to (+ to 2))))
            (setq from (+ from 3)))))))
  (list (length all)
        (secure-hash 'md5 (prin1-to-string (nreverse all)))
        (nreverse readable)))
"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(
            r#""OK (17844 \"f1588fbe1a848f8e0c9931d8e78509c3\" ((1 1) (3 0) (5 0) (7 0) (9 0) (11 1) (13 1) (15 1) (17 0) (19 0) (21 0) (23 0) (25 0) (27 0) (29 0) (31 0) (33 0) (35 1) (37 0) (39 0) (41 0) (43 0) (45 0) (1 1) (3 0) (5 0) (7 1) (9 1) (11 1) (13 1) (15 1) (17 1) (19 1) (21 0) (23 0) (25 0) (27 0) (29 0) (31 0) (33 0) (35 0) (37 1) (39 0) (41 0) (1 1) (3 0) (5 0) (7 0) (9 1) (11 1) (13 1) (15 0) (17 0) (19 0) (21 0) (23 0) (25 0) (27 0) (29 0) (31 1) (33 0) (35 0)))""#
        )
    );
}

/// GNU calls through `safe_calln`: errors are muted, a `throw` is not; a call
/// that changes the text or leaves `done` in place signals; ZV bounds the
/// call; the scan reads what the call propertized, except the character it
/// had already read.
#[test]
fn parse_partial_sexp_propertize_error_policy() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(let (log out)
  (with-temp-buffer
    (insert "(a b) ; c\n(d \"e\" f)\n")
    (setq-local parse-sexp-lookup-properties t)
    (setq-local syntax-propertize-function
                (lambda (s e) (push (list s e) log) (error "boom")))
    (setq syntax-propertize--done 3)
    (push (list 'muted
                (condition-case err (parse-partial-sexp 1 15) (error (list 'ERR err)))
                (point) syntax-propertize--done (reverse log))
          out)
    (setq log nil)
    (setq-local syntax-propertize-function
                (lambda (s e) (push (list s e) log)
                  (save-excursion (goto-char (point-max)) (insert "x"))))
    (setq syntax-propertize--done 3)
    (push (list 'modified
                (condition-case err (parse-partial-sexp 1 15) (error (list 'ERR err)))
                (buffer-size) (reverse log))
          out)
    (erase-buffer)
    (insert "(a b) ; c\n(d \"e\" f)\n")
    (setq log nil)
    (setq-local syntax-propertize-function (lambda (s e) (push (list s e) log)))
    (setq syntax-propertize--done 3)
    (let ((orig (symbol-function 'internal--syntax-propertize)))
      (unwind-protect
          (progn
            (fset 'internal--syntax-propertize
                  (lambda (pos) (push (list 'isp pos) log)))
            (push (list 'stuck
                        (condition-case err (parse-partial-sexp 1 15)
                          (error (list 'ERR err)))
                        (reverse log))
                  out))
        (fset 'internal--syntax-propertize orig)))
    (setq log nil)
    (setq-local syntax-propertize-function
                (lambda (s e) (push (list s e) log) (throw 'out 'thrown)))
    (setq syntax-propertize--done 3)
    (push (list 'throw (catch 'out (parse-partial-sexp 1 15))
                syntax-propertize--done (reverse log))
          out)
    (setq log nil)
    (setq-local syntax-propertize-function (lambda (s e) (push (list s e) log)))
    (setq-local parse-sexp-lookup-properties nil)
    (setq syntax-propertize--done 3)
    (push (list 'ignored (parse-partial-sexp 1 15) syntax-propertize--done (reverse log))
          out)
    (setq-local parse-sexp-lookup-properties t)
    (setq log nil)
    (setq syntax-propertize--done 3)
    (save-restriction
      (narrow-to-region 1 12)
      (push (list 'narrowed (parse-partial-sexp 1 12) syntax-propertize--done
                  (reverse log))
            out))
    (setq log nil)
    (setq-local syntax-propertize-function
                (lambda (s e) (push (list s e) log)
                  (setq-local parse-sexp-lookup-properties nil)))
    (setq syntax-propertize--done 3)
    (push (list 'switched-off (parse-partial-sexp 1 21) syntax-propertize--done
                parse-sexp-lookup-properties (reverse log))
          out)
    (setq-local parse-sexp-lookup-properties t)
    (setq log nil)
    (setq-local syntax-propertize-function
                (lambda (s e) (push (list s e) log)
                  (put-text-property 1 2 'syntax-table (string-to-syntax "."))
                  (put-text-property 12 13 'syntax-table (string-to-syntax "."))))
    (setq syntax-propertize--done 3)
    (push (list 'props-after (parse-partial-sexp 1 21) syntax-propertize--done
                (reverse log))
          out)
    (remove-text-properties 1 21 '(syntax-table nil))
    (setq log nil)
    (setq-local syntax-propertize-function
                (lambda (s e) (push (list s e) log)
                  (put-text-property 2 3 'syntax-table (string-to-syntax "("))
                  (put-text-property 3 4 'syntax-table (string-to-syntax "("))))
    (setq syntax-propertize--done 3)
    (push (list 'read-before-call (parse-partial-sexp 1 21) syntax-propertize--done
                (reverse log))
          out))
  (nreverse out))
"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(
            r#""OK ((muted (1 11 12 34 nil nil 0 nil 14 (11) nil) 15 21 ((1 21))) (modified (ERR (error \"internal--syntax-propertize modified the buffer!\")) 21 ((1 21))) (stuck (ERR (error \"internal--syntax-propertize did not move syntax-propertize--done\")) ((isp 4))) (throw thrown 21 ((1 21))) (ignored (1 11 12 34 nil nil 0 nil 14 (11) nil) 3 nil) (narrowed (1 11 nil nil nil nil 0 nil nil (11) nil) 12 ((1 12))) (switched-off (0 nil 11 nil nil nil 0 nil nil nil nil) 21 nil ((1 21))) (props-after (0 nil 11 nil nil nil 0 nil nil nil nil) 21 ((1 21))) (read-before-call (1 1 11 nil nil nil 0 nil nil (1) nil) 21 ((1 21))))""#
        )
    );
}
