use expect_test::expect;

use super::ParityBatchCase;

/// Wrapping existing code in a new form, by typing it: with point at the start
/// of `(message', the user types `(when request' and RET.  While the new form
/// is still open the defun is unbalanced and nothing below moves -- the package
/// will not reindent code it cannot parse -- and the moment the closing paren
/// is typed the whole defun is reindented and `(process request)' lands inside
/// the `when'.  That second step is the thing `electric-indent-mode' cannot do:
/// it is a line the user is not typing on.
fn typing_a_wrapper_form_reindents_the_lines_it_encloses_once_it_is_balanced() -> ParityBatchCase {
    ParityBatchCase::value(
        "typing_a_wrapper_form_reindents_the_lines_it_encloses_once_it_is_balanced",
        r##"(agi-test-with-buffer
 'emacs-lisp-mode agi-test-lisp-defun
 (search-forward "(message")
 (goto-char (match-beginning 0))
 (execute-kbd-macro (kbd "( w h e n SPC r e q u e s t RET"))
 (let ((typed (agi-test-text)))
   (agi-test-idle)
   (let ((still-open (agi-test-state)))
     (goto-char (point-max))
     (search-backward "(process request))")
     (end-of-line)
     (execute-kbd-macro (kbd ")"))
     (agi-test-idle)
     (list :typed typed
           :while-unbalanced still-open
           :after-closing (agi-test-state)))))"##,
        expect![[
            r#"OK (:typed "(defun handler (request)\n  (when request\n    (message \"start\")\n  (process request))\n" :while-unbalanced (:text "(defun handler (request)\n  (when request\n    (message \"start\")\n  (process request))\n" :point 46 :line 3 :column 4 :mode t :electric t) :after-closing (:text "(defun handler (request)\n  (when request\n    (message \"start\")\n    (process request)))\n" :point 87 :line 4 :column 23 :mode t :electric t))"#
        ]],
    )
}

fn deleting_the_enclosing_form_dedents_the_lines_it_contained() -> ParityBatchCase {
    ParityBatchCase::value(
        "deleting_the_enclosing_form_dedents_the_lines_it_contained",
        r##"(agi-test-with-buffer
 'emacs-lisp-mode agi-test-nested-lisp-defun
 (search-forward "(when request")
 (beginning-of-line)
 (execute-kbd-macro (kbd "C-k C-k"))
 (let ((killed (agi-test-state)))
   (agi-test-idle)
   (list :after-killing killed
         :after-idle (agi-test-state))))"##,
        expect![[
            r#"OK (:after-killing (:text "(defun handler (request)\n    (message \"start\")\n    (process request)))\n" :point 26 :line 2 :column 0 :mode t :electric t) :after-idle (:text "(defun handler (request)\n  (message \"start\")\n  (process request)))\n" :point 26 :line 2 :column 0 :mode t :electric t))"#
        ]],
    )
}

fn opening_a_block_in_a_c_buffer_reindents_the_statements_it_swallows() -> ParityBatchCase {
    ParityBatchCase::value(
        "opening_a_block_in_a_c_buffer_reindents_the_statements_it_swallows",
        r##"(agi-test-with-buffer
 'c-mode agi-test-c-function
 (search-forward "log(")
 (beginning-of-line)
 (execute-kbd-macro (kbd "i f SPC ( r e a d y ) SPC { RET"))
 (let ((typed (agi-test-text)))
   (agi-test-idle)
   (let ((opened (agi-test-state)))
     (goto-char (point-max))
     (search-backward "process(ready);")
     (end-of-line)
     (execute-kbd-macro (kbd "RET }"))
     (agi-test-idle)
     (list :typed typed
           :after-opening opened
           :after-closing (agi-test-state)))))"##,
        expect![[
            r#"OK (:typed "int handler(int ready) {\n  if (ready) {\n    log(\"start\");\n  process(ready);\n}\n" :after-opening (:text "int handler(int ready) {\n  if (ready) {\n    log(\"start\");\n    process(ready);\n}\n" :point 45 :line 3 :column 4 :mode t :electric t) :after-closing (:text "int handler(int ready) {\n  if (ready) {\n    log(\"start\");\n    process(ready);\n  }\n}\n" :point 82 :line 5 :column 3 :mode t :electric t))"#
        ]],
    )
}

fn backspace_on_the_leading_indentation_joins_the_line_instead_of_deleting_a_space()
-> ParityBatchCase {
    ParityBatchCase::value(
        "backspace_on_the_leading_indentation_joins_the_line_instead_of_deleting_a_space",
        r##"(list
 :after-indentation
 (agi-test-with-buffer
  'emacs-lisp-mode "(defun f ()\n  (message \"x\"))\n"
  (search-forward "(message")
  (goto-char (match-beginning 0))
  (let ((binding (key-binding [backspace])))
    (execute-kbd-macro (vector 'backspace))
    (let ((joined (agi-test-text)))
      (agi-test-idle)
      (list :binding binding :joined joined :after-idle (agi-test-text)))))
 :at-beginning-of-line
 (agi-test-with-buffer
  'emacs-lisp-mode "(defun f ()\n  (message \"x\"))\n"
  (search-forward "(message")
  (beginning-of-line)
  (let ((binding (key-binding [backspace])))
    (execute-kbd-macro (vector 'backspace))
    (let ((deleted (agi-test-text)))
      (agi-test-idle)
      (list :binding binding :deleted deleted :after-idle (agi-test-text))))))"##,
        expect![[
            r#"OK (:after-indentation (:binding delete-indentation :joined "(defun f () (message \"x\"))\n" :after-idle "(defun f () (message \"x\"))\n") :at-beginning-of-line (:binding nil :deleted "(defun f ()  (message \"x\"))\n" :after-idle "(defun f ()  (message \"x\"))\n"))"#
        ]],
    )
}

fn the_dont_indent_if_and_protected_commands_policies_keep_it_quiet() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_dont_indent_if_and_protected_commands_policies_keep_it_quiet",
        r##"(list
 :dont-indent-if
 (agi-test-with-buffer
  'emacs-lisp-mode agi-test-nested-lisp-defun
  (let ((aggressive-indent-dont-indent-if '((looking-at-p "[[:space:]]*(message"))))
    (search-forward "(when request")
    (beginning-of-line)
    (execute-kbd-macro (kbd "C-k C-k"))
    (agi-test-idle)
    (agi-test-text)))
 :without-that-guard
 (agi-test-with-buffer
  'emacs-lisp-mode agi-test-nested-lisp-defun
  (search-forward "(when request")
  (beginning-of-line)
  (execute-kbd-macro (kbd "C-k C-k"))
  (agi-test-idle)
  (agi-test-text))
 :protected-after-undo
 (agi-test-with-buffer
  'emacs-lisp-mode agi-test-nested-lisp-defun
  (search-forward "(when request")
  (beginning-of-line)
  (execute-kbd-macro (kbd "C-k C-k"))
  (agi-test-idle)
  (execute-kbd-macro (kbd "C-/"))
  (agi-test-idle)
  (list :last-command last-command
        :protected (memq 'undo aggressive-indent-protected-commands)
        :text (agi-test-text)))
 :unprotected-after-undo
 (agi-test-with-buffer
  'emacs-lisp-mode agi-test-nested-lisp-defun
  (let ((aggressive-indent-protected-commands nil))
    (search-forward "(when request")
    (beginning-of-line)
    (execute-kbd-macro (kbd "C-k C-k"))
    (agi-test-idle)
    (execute-kbd-macro (kbd "C-/"))
    (agi-test-idle)
    (list :last-command last-command
          :protected aggressive-indent-protected-commands
          :text (agi-test-text)))))"##,
        expect![[
            r#"OK (:dont-indent-if "(defun handler (request)\n    (message \"start\")\n    (process request)))\n" :without-that-guard "(defun handler (request)\n  (message \"start\")\n  (process request)))\n" :protected-after-undo (:last-command undo :protected (undo undo-tree-undo undo-tree-redo undo-tree-visualize undo-tree-visualize-undo undo-tree-visualize-redo whitespace-cleanup) :text "(defun handler (request)\n\n    (message \"start\")\n    (process request)))\n") :unprotected-after-undo (:last-command undo :protected nil :text "(defun handler (request)\n\n  (message \"start\")\n  (process request)))\n"))"#
        ]],
    )
}

fn one_undo_takes_back_both_the_edit_and_the_reindentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "one_undo_takes_back_both_the_edit_and_the_reindentation",
        r##"(agi-test-with-buffer
 'emacs-lisp-mode agi-test-nested-lisp-defun
 (search-forward "(when request")
 (beginning-of-line)
 (execute-kbd-macro (kbd "C-k C-k"))
 (agi-test-idle)
 (let ((reindented (agi-test-text)))
   (execute-kbd-macro (kbd "C-/"))
   (agi-test-idle)
   (list :original agi-test-nested-lisp-defun
         :after-edit reindented
         :after-one-undo (agi-test-text)
         :point (point))))"##,
        expect![[
            r#"OK (:original "(defun handler (request)\n  (when request\n    (message \"start\")\n    (process request)))\n" :after-edit "(defun handler (request)\n  (message \"start\")\n  (process request)))\n" :after-one-undo "(defun handler (request)\n\n    (message \"start\")\n    (process request)))\n" :point 26)"#
        ]],
    )
}

fn the_global_mode_skips_excluded_modes_while_the_local_command_does_not() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_global_mode_skips_excluded_modes_while_the_local_command_does_not",
        r##"(progn
  (global-aggressive-indent-mode 1)
  (let ((under-global
         (mapcar (lambda (mode)
                   (let ((buffer (generate-new-buffer "*agi-global*")))
                     (unwind-protect
                         (with-current-buffer buffer
                           (funcall mode)
                           (list mode
                                 aggressive-indent-mode
                                 (and (memq #'aggressive-indent--keep-track-of-changes
                                            after-change-functions)
                                      t)))
                       (kill-buffer buffer))))
                 '(emacs-lisp-mode c-mode text-mode fundamental-mode))))
    (global-aggressive-indent-mode -1)
    (list :excluded aggressive-indent-excluded-modes
          :under-global under-global
          :global-off (let ((buffer (generate-new-buffer "*agi-off*")))
                        (unwind-protect
                            (with-current-buffer buffer
                              (emacs-lisp-mode)
                              (list aggressive-indent-mode global-aggressive-indent-mode))
                          (kill-buffer buffer)))
          :local-in-excluded-mode
          (agi-test-with-buffer
           'text-mode "hello\n"
           (list aggressive-indent-mode
                 (and (memq #'aggressive-indent--keep-track-of-changes
                            after-change-functions)
                      t))))))"##,
        expect![
            "OK (:excluded (elm-mode haskell-mode inf-ruby-mode makefile-mode makefile-gmake-mode python-mode sql-interactive-mode text-mode yaml-mode) :under-global ((emacs-lisp-mode t t) (c-mode t t) (text-mode nil nil) (fundamental-mode nil nil)) :global-off (nil nil) :local-in-excluded-mode (t t))"
        ],
    )
}

fn saving_the_buffer_indents_what_was_typed_before_writing_it_to_disk() -> ParityBatchCase {
    ParityBatchCase::value(
        "saving_the_buffer_indents_what_was_typed_before_writing_it_to_disk",
        r##"(let ((path (agi-test-sandbox-file "project/handler.el")))
  (agi-test-with-buffer
   'emacs-lisp-mode ""
   (setq buffer-file-name path)
   (insert "(defun f (x)\n(when x\n(message \"hi\")))\n")
   (goto-char (point-max))
   (let ((before (agi-test-text)))
     (save-buffer)
     (list :before-save before
           :after-save (agi-test-text)
           :on-disk (agi-test-file-contents path)
           :modified (buffer-modified-p)
           :hook (and (memq #'aggressive-indent--process-changed-list-and-indent
                            before-save-hook)
                      t)))))"##,
        expect![[
            r#"OK (:before-save "(defun f (x)\n(when x\n(message \"hi\")))\n" :after-save "(defun f (x)\n  (when x\n    (message \"hi\")))\n" :on-disk "(defun f (x)\n  (when x\n    (message \"hi\")))\n" :modified nil :hook t)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        typing_a_wrapper_form_reindents_the_lines_it_encloses_once_it_is_balanced(),
        deleting_the_enclosing_form_dedents_the_lines_it_contained(),
        opening_a_block_in_a_c_buffer_reindents_the_statements_it_swallows(),
        backspace_on_the_leading_indentation_joins_the_line_instead_of_deleting_a_space(),
        the_dont_indent_if_and_protected_commands_policies_keep_it_quiet(),
        one_undo_takes_back_both_the_edit_and_the_reindentation(),
        the_global_mode_skips_excluded_modes_while_the_local_command_does_not(),
        saving_the_buffer_indents_what_was_typed_before_writing_it_to_disk(),
    ]
}
