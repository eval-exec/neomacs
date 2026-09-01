use expect_test::expect;

use super::ParityBatchCase;

fn free_and_bound_variables_analyze_let_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "free_and_bound_variables_analyze_let_forms",
        r####"
(let* ((form '(let ((x 1) (y 2)) (+ x y z)))
       (bound (sort (mapcar #'symbol-name (emr-el:bound-variables form))
                    #'string-lessp))
       (free (sort (mapcar #'symbol-name (emr-el:free-variables form))
                   #'string-lessp))
       (unquoted (sort (mapcar #'symbol-name
                               (emr-el:unquoted-symbols '(foo 'bar baz)))
                       #'string-lessp)))
  (list :bound bound
        :free free
        :unquoted unquoted
        :is-def (and (emr-el:definition? '(defun f () 1)) t)
        :is-var-def (and (emr-el:variable-definition? '(defvar x 1)) t)
        :not-def (emr-el:definition? '(+ 1 2))))
"####,
        expect![[
            r#"OK (:bound ("x" "y") :free ("z") :unquoted ("baz" "foo") :is-def t :is-var-def t :not-def nil)"#
        ]],
    )
}

fn toggle_let_star_and_eval_and_replace_mutate_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "toggle_let_star_and_eval_and_replace_mutate_buffer",
        r####"
(neomacs-emr-test-with-elisp
 "(let* ((x 1))
   (+ x 2))\n"
 (lambda ()
   (search-forward "+")
   (emr-el-toggle-let*)
   (let ((after-toggle (buffer-string)))
     (erase-buffer)
     (insert "(+ 1 2)\n")
     (goto-char (point-min))
     (search-forward "+")
     (backward-char)
     (emr-el-eval-and-replace)
     (list :toggle (string-trim after-toggle)
           :eval-replace (string-trim (buffer-string))))))
"####,
        expect![[r#"OK (:toggle "(let ((x 1))\n   (+ x 2))" :eval-replace "3")"#]],
    )
}

fn extract_variable_and_inline_round_trip() -> ParityBatchCase {
    ParityBatchCase::value(
        "extract_variable_and_inline_round_trip",
        r####"
(neomacs-emr-test-with-elisp
 "(defun demo ()
  (+ 1 2))\n"
 (lambda ()
   (search-forward "+ 1 2")
   (backward-up-list)
   (cl-letf (((symbol-function 'read-string)
              (lambda (_prompt &optional _initial) "sum")))
     (emr-el-extract-variable "sum"))
   (let ((extracted (buffer-string)))
     (goto-char (point-min))
     (search-forward "defvar sum")
     (forward-line 0)
     (emr-el-inline-variable)
     (list :extracted (string-trim extracted)
           :inlined (string-trim (buffer-string))))))
"####,
        expect![[
            r#"OK (:extracted "(defvar sum (+ 1 2))\n\n(defun demo ()\n  sum)" :inlined "(defun demo ()\n  (+ 1 2))")"#
        ]],
    )
}

fn declare_command_and_make_popup_respects_predicate() -> ParityBatchCase {
    ParityBatchCase::value(
        "declare_command_and_make_popup_respects_predicate",
        r####"
(let ((emr:refactor-commands (make-hash-table :test 'equal)))
  (emr-declare-command 'identity
    :modes 'emacs-lisp-mode
    :title "always"
    :description "yes"
    :predicate (lambda () t))
  (emr-declare-command 'ignore
    :modes 'emacs-lisp-mode
    :title "never"
    :description "no"
    :predicate (lambda () nil))
  (emr-declare-command 'list
    :modes 'c-mode
    :title "wrong-mode"
    :description "no"
    :predicate (lambda () t))
  (with-temp-buffer
    (emacs-lisp-mode)
    (let* ((specs (emr:hash-values emr:refactor-commands))
           (popups (-keep #'emr:make-popup specs)))
      (list :spec-count (length specs)
            :popup-titles (mapcar #'popup-item-value popups)
            :available-count (length popups)))))
"####,
        expect!["OK (:spec-count 3 :popup-titles (identity) :available-count 1)"],
    )
}

fn insert_above_defun_places_text_before_top_level_form() -> ParityBatchCase {
    ParityBatchCase::value(
        "insert_above_defun_places_text_before_top_level_form",
        r####"
(neomacs-emr-test-with-elisp
 "(defun demo ()
  1)\n"
 (lambda ()
   (search-forward "1")
   (emr-insert-above-defun "(defvar marker t)")
   (list :text (string-trim (buffer-string))
         :starts-with-defvar
         (and (string-match-p "\\`(defvar marker t)"
                              (string-trim (buffer-string)))
              t)
         :still-has-defun
         (and (string-match-p "(defun demo" (buffer-string)) t))))
"####,
        expect![[
            r#"OK (:text "(defvar marker t)\n\n(defun demo ()\n  1)" :starts-with-defvar t :still-has-defun t)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        free_and_bound_variables_analyze_let_forms(),
        toggle_let_star_and_eval_and_replace_mutate_buffer(),
        extract_variable_and_inline_round_trip(),
        declare_command_and_make_popup_respects_predicate(),
        insert_above_defun_places_text_before_top_level_form(),
    ]
}
