use expect_test::expect;

use super::ParityBatchCase;

fn opens_and_fontifies_a_real_smt_model() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "z3-mode-open"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (path (expand-file-name "capacity-plan.smt2" sandbox))
       buffer result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory sandbox t)
        (with-temp-file path
          (insert
           "; Capacity planning model: assert and check-sat are documentation.\n"
           "(set-logic QF_LIA)\n"
           "(set-option :produce-models true)\n"
           "(declare-const workers Int)\n"
           "(assert (and (>= workers #x2A) (< workers #b111111)))\n"
           "(echo \"check-sat :inside-string #xFF\")\n"
           "(assertion workers)\n"
           "(check-sat)\n"
           "(get-model)\n"))
        (setq buffer (find-file-noselect path))
        (with-current-buffer buffer
          (font-lock-ensure)
          (setq result
                (list
                 :file buffer-file-name
                 :mode major-mode
                 :mode-name mode-name
                 :derived
                 (list (derived-mode-p 'lisp-mode)
                       (derived-mode-p 'prog-mode))
                 :key (key-binding (kbd "C-c C-c"))
                 :font-lock
                 (list
                  (equal font-lock-defaults z3-font-lock-defaults)
                  (length (car font-lock-defaults)))
                 :faces (neomacs-melpa-z3-mode--face-runs)
                 :contexts
                 (mapcar
                  #'neomacs-melpa-z3-mode--face-segments
                  '("assertion"
                    "Capacity planning model: assert and check-sat"
                    "check-sat :inside-string #xFF"))
                 :auto-mode
                 (cdr (assoc "\\.smt[2]?$" auto-mode-alist))
                 :modified (buffer-modified-p)))
          result))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t))))
"####;
    let expect = expect![[
        r##"OK (:file "[ORACLE-SANDBOX]/z3-mode-open/capacity-plan.smt2" :mode z3-mode :mode-name "Z3/SMT2" :derived (lisp-mode prog-mode) :key z3-execute-region :font-lock (t 3) :faces (("; " font-lock-comment-delimiter-face 1 3) ("Capacity planning model: assert and check-sat are documentation.\n" font-lock-comment-face 3 68) ("set-logic" font-lock-keyword-face 69 78) ("set-option" font-lock-keyword-face 88 98) (":produce-models" font-lock-builtin-face 99 114) ("declare-const" font-lock-keyword-face 122 135) ("assert" font-lock-keyword-face 150 156) ("#x2A" font-lock-constant-face 174 178) ("#b111111" font-lock-constant-face 191 199) ("echo" font-lock-keyword-face 204 208) ("\"check-sat :inside-string #xFF\"" font-lock-string-face 209 240) ("check-sat" font-lock-keyword-face 263 272) ("get-model" font-lock-keyword-face 275 284)) :contexts (("assertion" 243 252 (("assertion" nil 0 9))) ("Capacity planning model: assert and check-sat" 3 48 (("Capacity planning model: assert and check-sat" font-lock-comment-face 0 45))) ("check-sat :inside-string #xFF" 210 239 (("check-sat :inside-string #xFF" font-lock-string-face 0 29)))) :auto-mode z3-mode :modified nil)"##
    ]];
    ParityBatchCase::value("opens_and_fontifies_a_real_smt_model", elisp_form, expect)
}

fn executes_the_whole_model_selected_query_and_failing_command() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "z3-mode-execute"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (solver (expand-file-name "z3-parity" sandbox))
       (calls-file (expand-file-name "solver-calls.log" sandbox))
       (process-environment
        (cons (concat "NEOMACS_Z3_CALLS=" calls-file)
              process-environment))
       (default-directory (file-name-as-directory sandbox))
       (z3-solver-cmd solver)
       (source
        (concat
         "(set-logic QF_LIA)\n"
         "(declare-const workers Int)\n"
         "(assert (> workers 8))\n"
         "(check-sat)\n"
         "(get-model)\n"))
       buffer output-buffer whole selected failed result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory sandbox t)
        (neomacs-melpa-z3-mode--write-executable
         solver
         "input=$(cat)\nprintf 'ARGS' >> \"$NEOMACS_Z3_CALLS\"\nfor arg in \"$@\"; do printf ' <%s>' \"$arg\" >> \"$NEOMACS_Z3_CALLS\"; done\nprintf '\\nPWD <%s>\\nINPUT <<%s>>\\n' \"$PWD\" \"$input\" >> \"$NEOMACS_Z3_CALLS\"\ncase \"$input\" in\n  *'(bad-command)'*) printf '%s\\n' 'solver error: unknown command bad-command'; exit 7 ;;\n  *'(declare-const workers Int)'*) printf '%s\\n' 'sat' '((workers 9))' ;;\n  *) printf '%s\\n' 'unknown' '(reason-unknown \"incomplete query\")' ;;\nesac\n")
        (setq buffer (generate-new-buffer " *z3-mode-execute-parity*"))
        (with-current-buffer buffer
          (insert source)
          (z3-mode)
          (goto-char (point-min))
          (setq whole
                (list
                 (call-interactively (key-binding (kbd "C-c C-c")))
                 (buffer-substring-no-properties (point-min) (point-max))))
          (setq output-buffer (get-buffer "*Shell Command Output*")
                whole
                (append
                 whole
                 (list
                  (and output-buffer
                       (with-current-buffer output-buffer
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))))
          (goto-char (point-min))
          (search-forward "(check-sat)")
          (let ((start (match-beginning 0)))
            (search-forward "(get-model)")
            (let ((transient-mark-mode t))
              (goto-char start)
              (push-mark (match-end 0) t t)
              (setq selected
                    (list
                     (call-interactively (key-binding (kbd "C-c C-c")))
                     (buffer-substring-no-properties
                      (region-beginning) (region-end))
                     (and output-buffer
                          (with-current-buffer output-buffer
                            (buffer-substring-no-properties
                             (point-min) (point-max))))))))
          (deactivate-mark)
          (goto-char (point-max))
          (insert "(bad-command)\n")
          (goto-char (point-min))
          (search-forward "(bad-command)")
          (let ((transient-mark-mode t))
            (push-mark (match-beginning 0) t t)
            (setq failed
                  (list
                   (call-interactively (key-binding (kbd "C-c C-c")))
                   (buffer-substring-no-properties
                    (region-beginning) (region-end))
                   (and output-buffer
                        (with-current-buffer output-buffer
                          (buffer-substring-no-properties
                           (point-min) (point-max)))))))
          (setq result
                (list
                 :whole whole
                 :selected selected
                 :failed failed
                 :source (buffer-substring-no-properties
                          (point-min) (point-max))
                 :calls
                 (neomacs-melpa-z3-mode--file-string calls-file)))
          result))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))
    (when (buffer-live-p output-buffer)
      (kill-buffer output-buffer))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t))))
"####;
    let expect = expect![[
        r#"OK (:whole (0 "(set-logic QF_LIA)\n(declare-const workers Int)\n(assert (> workers 8))\n(check-sat)\n(get-model)\n" "sat\n((workers 9))\n") :selected (0 "(check-sat)\n(get-model)" "unknown\n(reason-unknown \"incomplete query\")\n") :failed (7 "(bad-command)" "solver error: unknown command bad-command\n") :source "(set-logic QF_LIA)\n(declare-const workers Int)\n(assert (> workers 8))\n(check-sat)\n(get-model)\n(bad-command)\n" :calls "ARGS <-in>\nPWD <[ORACLE-SANDBOX]/z3-mode-execute>\nINPUT <<(set-logic QF_LIA)\n(declare-const workers Int)\n(assert (> workers 8))\n(check-sat)\n(get-model)>>\nARGS <-in>\nPWD <[ORACLE-SANDBOX]/z3-mode-execute>\nINPUT <<(check-sat)\n(get-model)>>\nARGS <-in>\nPWD <[ORACLE-SANDBOX]/z3-mode-execute>\nINPUT <<(bad-command)>>\n")"#
    ]];
    ParityBatchCase::value(
        "executes_the_whole_model_selected_query_and_failing_command",
        elisp_form,
        expect,
    )
}

fn reports_a_real_solver_diagnostic_through_the_registered_flycheck_checker() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "z3-mode-flycheck"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (path (expand-file-name "broken-model.smt2" sandbox))
       (solver (expand-file-name "z3-parity" sandbox))
       (calls-file (expand-file-name "flycheck-calls.log" sandbox))
       (process-environment
        (append
         (list
          (concat "PATH=" sandbox path-separator (getenv "PATH"))
          (concat "NEOMACS_Z3_CALLS=" calls-file))
         process-environment))
       (exec-path (cons sandbox exec-path))
       buffer result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory sandbox t)
        (neomacs-melpa-z3-mode--write-executable
         solver
         "source_content_match=no\nif grep -F '(assert (> release-limit capacity))' \"$3\" >/dev/null; then source_content_match=yes; fi\nsource_path_match=no\ncase \"$3\" in \"$TMPDIR\"/flycheck*/broken-model.smt2) source_path_match=yes ;; esac\nsource_root=$(dirname \"$(dirname \"$3\")\")\nsource_name=$(basename \"$3\")\nprintf 'PWD <%s>\\nARGS <%s> <%s> SOURCE <%s/[FLYCHECK-DIR]/%s> PATH-MATCH <%s> CONTENT-MATCH <%s>\\n' \"$PWD\" \"$1\" \"$2\" \"$source_root\" \"$source_name\" \"$source_path_match\" \"$source_content_match\" >> \"$NEOMACS_Z3_CALLS\"\nprintf '%s\\n' 'error \"line 4 column 12: unexpected identifier release-limit\")'\nexit 1\n")
        (with-temp-file path
          (insert
           "(set-logic QF_LIA)\n"
           "(declare-const workers Int)\n"
           "(declare-const capacity Int)\n"
           "(assert (> release-limit capacity))\n"
           "(check-sat)\n"))
        (setq buffer (find-file-noselect path))
        (with-current-buffer buffer
          (flycheck-select-checker 'z3-smt2-lint)
          (flycheck-mode 1)
          (neomacs-melpa-z3-mode--wait-for-flycheck)
          (setq result
                (list
                 :mode major-mode
                 :checker flycheck-checker
                 :registered
                 (list
                  (flycheck-checker-get 'z3-smt2-lint 'command)
                  (flycheck-checker-get 'z3-smt2-lint 'modes))
                 :status flycheck-last-status-change
                 :diagnostics (neomacs-melpa-z3-mode--diagnostics)
                 :calls
                 (neomacs-melpa-z3-mode--file-string calls-file)
                 :source (buffer-substring-no-properties
                          (point-min) (point-max))
                 :modified (buffer-modified-p)))
          (flycheck-mode -1)
          result))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (flycheck-mode -1)
        (set-buffer-modified-p nil))
      (kill-buffer buffer))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t))))
"####;
    let expect = expect![[
        r#"OK (:mode z3-mode :checker z3-smt2-lint :registered (("z3-parity" "-v:1" "-smt2" source) (z3-mode)) :status finished :diagnostics ((4 12 error z3-smt2-lint "unexpected identifier release-limit")) :calls "PWD <[ORACLE-SANDBOX]/z3-mode-flycheck>\nARGS <-v:1> <-smt2> SOURCE <[ORACLE-TMPDIR]/[FLYCHECK-DIR]/broken-model.smt2> PATH-MATCH <yes> CONTENT-MATCH <yes>\n" :source "(set-logic QF_LIA)\n(declare-const workers Int)\n(declare-const capacity Int)\n(assert (> release-limit capacity))\n(check-sat)\n" :modified nil)"#
    ]];
    ParityBatchCase::value(
        "reports_a_real_solver_diagnostic_through_the_registered_flycheck_checker",
        elisp_form,
        expect,
    )
}

fn edits_indents_and_repairs_a_legacy_smt_model() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "z3-mode-edit"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (path (expand-file-name "legacy-capacity.smt" sandbox))
       buffer result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory sandbox t)
        (with-temp-file path
          (insert
           "(set-logic QF_LIA)\n"
           "(declare-const workers Int)\n"
           "(declare-const capacity Int)\n"
           "(assert\n"
           "(and (> workers 8)\n"
           "(< workers\n"
           "capacity)))\n"
           "(check-sat\n"))
        (setq buffer (find-file-noselect path))
        (with-current-buffer buffer
          (let ((before
                 (buffer-substring-no-properties
                  (point-min) (point-max))))
            (indent-region (point-min) (point-max))
            (let* ((indented
                    (buffer-substring-no-properties
                     (point-min) (point-max)))
                   (depth-before-repair
                    (car (syntax-ppss (point-max))))
                   (unmatched
                    (condition-case err
                        (progn
                          (check-parens)
                          :balanced)
                      (error
                       (list (car err) (cdr err) (point))))))
              (goto-char (point-max))
              (insert ")\n")
              (indent-region (point-min) (point-max))
              (let ((repaired
                     (buffer-substring-no-properties
                      (point-min) (point-max)))
                    navigation commented-line final)
                (goto-char (point-min))
                (search-forward "(assert")
                (goto-char (match-beginning 0))
                (let ((start (point)))
                  (forward-sexp)
                  (setq navigation
                        (list
                         start
                         (point)
                         (buffer-substring-no-properties start (point)))))
                (goto-char (point-min))
                (comment-region
                 (line-beginning-position)
                 (line-end-position))
                (setq commented-line
                      (buffer-substring-no-properties
                       (line-beginning-position)
                       (line-end-position)))
                (uncomment-region
                 (line-beginning-position)
                 (line-end-position))
                (setq final
                      (buffer-substring-no-properties
                       (point-min) (point-max)))
                (setq result
                      (list
                       :file buffer-file-name
                       :mode major-mode
                       :derived
                       (list
                        (derived-mode-p 'lisp-mode)
                        (derived-mode-p 'prog-mode))
                       :indent-line indent-line-function
                       :indent-command
                       (key-binding (kbd "C-M-q"))
                       :before before
                       :indented indented
                       :depth-before-repair depth-before-repair
                       :unmatched unmatched
                       :repaired repaired
                       :balanced
                       (progn (check-parens) t)
                       :depth-after-repair
                       (car (syntax-ppss (point-max)))
                       :navigation navigation
                       :commented-line commented-line
                       :final final
                       :modified (buffer-modified-p)))
                result)))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t))))
"####;
    let expect = expect![[
        r##"OK (:file "[ORACLE-SANDBOX]/z3-mode-edit/legacy-capacity.smt" :mode z3-mode :derived (lisp-mode prog-mode) :indent-line lisp-indent-line :indent-command indent-sexp :before "(set-logic QF_LIA)\n(declare-const workers Int)\n(declare-const capacity Int)\n(assert\n(and (> workers 8)\n(< workers\ncapacity)))\n(check-sat\n" :indented "(set-logic QF_LIA)\n(declare-const workers Int)\n(declare-const capacity Int)\n(assert\n (and (> workers 8)\n      (< workers\n         capacity)))\n(check-sat\n" :depth-before-repair 1 :unmatched (user-error ("Unmatched bracket or quote") 143) :repaired "(set-logic QF_LIA)\n(declare-const workers Int)\n(declare-const capacity Int)\n(assert\n (and (> workers 8)\n      (< workers\n         capacity)))\n(check-sat\n )\n" :balanced t :depth-after-repair 0 :navigation (77 142 "(assert\n (and (> workers 8)\n      (< workers\n         capacity)))") :commented-line ";; (set-logic QF_LIA)" :final "(set-logic QF_LIA)\n(declare-const workers Int)\n(declare-const capacity Int)\n(assert\n (and (> workers 8)\n      (< workers\n         capacity)))\n(check-sat\n )\n" :modified t)"##
    ]];
    ParityBatchCase::value(
        "edits_indents_and_repairs_a_legacy_smt_model",
        elisp_form,
        expect,
    )
}

fn fontifies_the_complete_command_vocabulary_and_boundary_contexts() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (dolist (command z3-keywords)
    (insert "(" command ")\n"))
  (insert
   "; assert-soft check-sat #xAA :named stay in this comment.\n"
   "(echo \"assert-soft check-sat #b101 :named\")\n"
   "(assert-soft-extra proposition)\n"
   "(assertion proposition)\n"
   "(:release-goal true)\n"
   "(#xF0FA #b010)\n")
  (z3-mode)
  (font-lock-ensure)
  (list
   :commands
   (mapcar
    (lambda (command)
      (neomacs-melpa-z3-mode--face-segments
       (concat "(" command ")")))
    z3-keywords)
   :contexts
   (mapcar
    #'neomacs-melpa-z3-mode--face-segments
    '("assert-soft-extra"
      "assertion proposition"
      "assert-soft check-sat #xAA :named"
      "assert-soft check-sat #b101 :named"))
   :symbols
   (mapcar
    #'neomacs-melpa-z3-mode--face-segments
    '(":release-goal" "#xF0FA" "#b010"))
   :lines (line-number-at-pos (point-max))))
"####;
    let expect = expect![[
        r##"OK (:commands (("(apply)" 1 8 (("(" nil 0 1) ("apply" font-lock-keyword-face 1 6) (")" nil 6 7))) ("(assert)" 9 17 (("(" nil 0 1) ("assert" font-lock-keyword-face 1 7) (")" nil 7 8))) ("(assert-soft)" 18 31 (("(" nil 0 1) ("assert-soft" font-lock-keyword-face 1 12) (")" nil 12 13))) ("(check-sat)" 32 43 (("(" nil 0 1) ("check-sat" font-lock-keyword-face 1 10) (")" nil 10 11))) ("(check-sat-using)" 44 61 (("(" nil 0 1) ("check-sat-using" font-lock-keyword-face 1 16) (")" nil 16 17))) ("(compute-interpolant)" 62 83 (("(" nil 0 1) ("compute-interpolant" font-lock-keyword-face 1 20) (")" nil 20 21))) ("(declare-const)" 84 99 (("(" nil 0 1) ("declare-const" font-lock-keyword-face 1 14) (")" nil 14 15))) ("(declare-datatypes)" 100 119 (("(" nil 0 1) ("declare-datatypes" font-lock-keyword-face 1 18) (")" nil 18 19))) ("(declare-fun)" 120 133 (("(" nil 0 1) ("declare-fun" font-lock-keyword-face 1 12) (")" nil 12 13))) ("(declare-map)" 134 147 (("(" nil 0 1) ("declare-map" font-lock-keyword-face 1 12) (")" nil 12 13))) ("(declare-rel)" 148 161 (("(" nil 0 1) ("declare-rel" font-lock-keyword-face 1 12) (")" nil 12 13))) ("(declare-sort)" 162 176 (("(" nil 0 1) ("declare-sort" font-lock-keyword-face 1 13) (")" nil 13 14))) ("(declare-tactic)" 177 193 (("(" nil 0 1) ("declare-tactic" font-lock-keyword-face 1 15) (")" nil 15 16))) ("(define-sort)" 194 207 (("(" nil 0 1) ("define-sort" font-lock-keyword-face 1 12) (")" nil 12 13))) ("(display)" 208 217 (("(" nil 0 1) ("display" font-lock-keyword-face 1 8) (")" nil 8 9))) ("(echo)" 218 224 (("(" nil 0 1) ("echo" font-lock-keyword-face 1 5) (")" nil 5 6))) ("(eval)" 225 231 (("(" nil 0 1) ("eval" font-lock-keyword-face 1 5) (")" nil 5 6))) ("(exit)" 232 238 (("(" nil 0 1) ("exit" font-lock-keyword-face 1 5) (")" nil 5 6))) ("(fixedpoint-pop)" 239 255 (("(" nil 0 1) ("fixedpoint-pop" font-lock-keyword-face 1 15) (")" nil 15 16))) ("(fixedpoint-push)" 256 273 (("(" nil 0 1) ("fixedpoint-push" font-lock-keyword-face 1 16) (")" nil 16 17))) ("(get-assertions)" 274 290 (("(" nil 0 1) ("get-assertions" font-lock-keyword-face 1 15) (")" nil 15 16))) ("(get-assignment)" 291 307 (("(" nil 0 1) ("get-assignment" font-lock-keyword-face 1 15) (")" nil 15 16))) ("(get-info)" 308 318 (("(" nil 0 1) ("get-info" font-lock-keyword-face 1 9) (")" nil 9 10))) ("(get-interpolant)" 319 336 (("(" nil 0 1) ("get-interpolant" font-lock-keyword-face 1 16) (")" nil 16 17))) ("(get-model)" 337 348 (("(" nil 0 1) ("get-model" font-lock-keyword-face 1 10) (")" nil 10 11))) ("(get-option)" 349 361 (("(" nil 0 1) ("get-option" font-lock-keyword-face 1 11) (")" nil 11 12))) ("(get-proof)" 362 373 (("(" nil 0 1) ("get-proof" font-lock-keyword-face 1 10) (")" nil 10 11))) ("(get-unsat-core)" 374 390 (("(" nil 0 1) ("get-unsat-core" font-lock-keyword-face 1 15) (")" nil 15 16))) ("(get-user-tactics)" 391 409 (("(" nil 0 1) ("get-user-tactics" font-lock-keyword-face 1 17) (")" nil 17 18))) ("(get-value)" 410 421 (("(" nil 0 1) ("get-value" font-lock-keyword-face 1 10) (")" nil 10 11))) ("(help)" 422 428 (("(" nil 0 1) ("help" font-lock-keyword-face 1 5) (")" nil 5 6))) ("(help-tactic)" 429 442 (("(" nil 0 1) ("help-tactic" font-lock-keyword-face 1 12) (")" nil 12 13))) ("(labels)" 443 451 (("(" nil 0 1) ("labels" font-lock-keyword-face 1 7) (")" nil 7 8))) ("(maximize)" 452 462 (("(" nil 0 1) ("maximize" font-lock-keyword-face 1 9) (")" nil 9 10))) ("(minimize)" 463 473 (("(" nil 0 1) ("minimize" font-lock-keyword-face 1 9) (")" nil 9 10))) ("(pop)" 474 479 (("(" nil 0 1) ("pop" font-lock-keyword-face 1 4) (")" nil 4 5))) ("(push)" 480 486 (("(" nil 0 1) ("push" font-lock-keyword-face 1 5) (")" nil 5 6))) ("(query)" 487 494 (("(" nil 0 1) ("query" font-lock-keyword-face 1 6) (")" nil 6 7))) ("(reset)" 495 502 (("(" nil 0 1) ("reset" font-lock-keyword-face 1 6) (")" nil 6 7))) ("(rule)" 503 509 (("(" nil 0 1) ("rule" font-lock-keyword-face 1 5) (")" nil 5 6))) ("(set-info)" 510 520 (("(" nil 0 1) ("set-info" font-lock-keyword-face 1 9) (")" nil 9 10))) ("(set-logic)" 521 532 (("(" nil 0 1) ("set-logic" font-lock-keyword-face 1 10) (")" nil 10 11))) ("(set-option)" 533 545 (("(" nil 0 1) ("set-option" font-lock-keyword-face 1 11) (")" nil 11 12))) ("(simplify)" 546 556 (("(" nil 0 1) ("simplify" font-lock-keyword-face 1 9) (")" nil 9 10)))) :contexts (("assert-soft-extra" 660 677 (("assert-soft" font-lock-keyword-face 0 11) ("-extra" nil 11 17))) ("assertion proposition" 692 713 (("assertion proposition" nil 0 21))) ("assert-soft check-sat #xAA :named" 559 592 (("assert-soft check-sat #xAA :named" font-lock-comment-face 0 33))) ("assert-soft check-sat #b101 :named" 622 656 (("assert-soft check-sat #b101 :named" font-lock-string-face 0 34)))) :symbols ((":release-goal" 716 729 ((":release-goal" font-lock-builtin-face 0 13))) ("#xF0FA" 737 743 (("#xF0FA" font-lock-constant-face 0 6))) ("#b010" 744 749 (("#b010" font-lock-constant-face 0 5)))) :lines 51)"##
    ]];
    ParityBatchCase::value(
        "fontifies_the_complete_command_vocabulary_and_boundary_contexts",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opens_and_fontifies_a_real_smt_model(),
        executes_the_whole_model_selected_query_and_failing_command(),
        reports_a_real_solver_diagnostic_through_the_registered_flycheck_checker(),
        edits_indents_and_repairs_a_legacy_smt_model(),
        fontifies_the_complete_command_vocabulary_and_boundary_contexts(),
    ]
}
