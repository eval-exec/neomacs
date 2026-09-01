use std::time::Duration;

use expect_test::expect;

use crate::{CLEAN_AINDENT_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'advice)
(require 'clean-aindent-mode)

(clean-aindent-mode -1)

(defun neomacs-clean-aindent-test-with-mode (simple function)
  (let ((clean-aindent-is-simple-indent simple))
    (unwind-protect
        (progn
          (clean-aindent-mode 1)
          (funcall function))
      (clean-aindent-mode -1)
      (setq clean-aindent--last-indent nil
            clean-aindent--last-indent-len 0))))

(defun neomacs-clean-aindent-test-state ()
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :column (current-column)
        :tracked clean-aindent--last-indent
        :tracked-length clean-aindent--last-indent-len
        :tracked-local (local-variable-p 'clean-aindent--last-indent)
        :length-local (local-variable-p 'clean-aindent--last-indent-len)))
"####;

fn smart_indent_trims_an_abandoned_language_indent_after_the_next_command() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-clean-aindent-test-with-mode
 nil
 (lambda ()
   (with-temp-buffer
     (emacs-lisp-mode)
     (insert "(let ((release-id 42))")
     (newline-and-indent)
     (let ((before (neomacs-clean-aindent-test-state)))
       (forward-line -1)
       (run-hooks 'post-command-hook)
       (list :before before
             :after (neomacs-clean-aindent-test-state))))))
"####;
    let expected = expect![[
        r#"OK (:before (:text "(let ((release-id 42))\n  " :point 26 :column 2 :tracked 26 :tracked-length 2 :tracked-local t :length-local t) :after (:text "(let ((release-id 42))\n" :point 1 :column 0 :tracked nil :tracked-length 2 :tracked-local t :length-local t))"#
    ]];
    ParityBatchCase::value(
        "smart_indent_trims_an_abandoned_language_indent_after_the_next_command",
        elisp_form,
        expected,
    )
}

fn simple_indent_cleans_trailing_space_and_reuses_the_nearest_nonblank_indent() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-clean-aindent-test-with-mode
 t
 (lambda ()
   (with-temp-buffer
     (insert "  deploy --region us-east-1   ")
     (newline-and-indent)
     (let ((first (neomacs-clean-aindent-test-state)))
       (newline-and-indent)
       (let ((second (neomacs-clean-aindent-test-state)))
         (goto-char (point-min))
         (run-hooks 'post-command-hook)
         (list :first first
               :second second
               :after-leaving (neomacs-clean-aindent-test-state)))))))
"####;
    let expected = expect![[
        r#"OK (:first (:text "  deploy --region us-east-1\n  " :point 31 :column 2 :tracked 31 :tracked-length 2 :tracked-local t :length-local t) :second (:text "  deploy --region us-east-1\n\n  " :point 32 :column 2 :tracked 32 :tracked-length 2 :tracked-local t :length-local t) :after-leaving (:text "  deploy --region us-east-1\n\n" :point 1 :column 0 :tracked nil :tracked-length 2 :tracked-local t :length-local t))"#
    ]];
    ParityBatchCase::value(
        "simple_indent_cleans_trailing_space_and_reuses_the_nearest_nonblank_indent",
        elisp_form,
        expected,
    )
}

fn entering_real_code_preserves_the_indented_line_when_point_moves_away() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-clean-aindent-test-with-mode
 nil
 (lambda ()
   (with-temp-buffer
     (emacs-lisp-mode)
     (insert "(progn")
     (newline-and-indent)
     (insert "(message \"shipping Ω\")")
     (let ((before-leaving (neomacs-clean-aindent-test-state)))
       (goto-char (point-min))
       (run-hooks 'post-command-hook)
       (list :before-leaving before-leaving
             :after-leaving (neomacs-clean-aindent-test-state))))))
"####;
    let expected = expect![[
        r#"OK (:before-leaving (:text "(progn\n  (message \"shipping Ω\")" :point 32 :column 24 :tracked 10 :tracked-length 2 :tracked-local t :length-local t) :after-leaving (:text "(progn\n  (message \"shipping Ω\")" :point 1 :column 0 :tracked nil :tracked-length 2 :tracked-local t :length-local t))"#
    ]];
    ParityBatchCase::value(
        "entering_real_code_preserves_the_indented_line_when_point_moves_away",
        elisp_form,
        expected,
    )
}

fn abandoned_indent_cleanup_can_be_undone_without_losing_typed_code() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-clean-aindent-test-with-mode
 nil
 (lambda ()
   (with-temp-buffer
     (emacs-lisp-mode)
     (buffer-enable-undo)
     (insert "(when release-ready")
     (newline-and-indent)
     (undo-boundary)
     (goto-char (point-min))
     (run-hooks 'post-command-hook)
     (let ((after-cleanup (buffer-string)))
       (undo-start)
       (undo-more 1)
       (list :after-cleanup after-cleanup
             :after-undo (buffer-string)
             :point (point)
             :column (current-column)
             :tracked clean-aindent--last-indent)))))
"####;
    let expected = expect![[
        r#"OK (:after-cleanup "(when release-ready\n" :after-undo "(when release-ready\n  " :point 23 :column 2 :tracked nil)"#
    ]];
    ParityBatchCase::value(
        "abandoned_indent_cleanup_can_be_undone_without_losing_typed_code",
        elisp_form,
        expected,
    )
}

fn repeated_backspace_unindent_walks_real_nested_source_to_each_outer_level() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-clean-aindent-test-with-mode
 nil
 (lambda ()
   (with-temp-buffer
     (insert "root\n  child\n    grandchild\n        deploy-production")
     (goto-char (point-max))
     (beginning-of-line)
     (back-to-indentation)
     (let (states)
       (dotimes (_ 3)
         (clean-aindent--bsunindent 1)
         (push (list :text (buffer-string)
                     :point (point)
                     :column (current-column)
                     :indent (current-indentation))
               states))
       (nreverse states)))))
"####;
    let expected = expect![[
        r#"OK ((:text "root\n  child\n    grandchild\n    deploy-production" :point 33 :column 4 :indent 4) (:text "root\n  child\n    grandchild\n  deploy-production" :point 31 :column 2 :indent 2) (:text "root\n  child\n    grandchild\ndeploy-production" :point 29 :column 0 :indent 0))"#
    ]];
    ParityBatchCase::value(
        "repeated_backspace_unindent_walks_real_nested_source_to_each_outer_level",
        elisp_form,
        expected,
    )
}

fn tabbed_indentation_is_removed_by_visual_column_without_damaging_text() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-clean-aindent-test-with-mode
 nil
 (lambda ()
   (with-temp-buffer
     (let ((tab-width 4)
           (indent-tabs-mode t)
           (backward-delete-char-untabify-method 'untabify))
       (insert "root\n\tchild\n\t  deploy-production")
       (goto-char (point-max))
       (beginning-of-line)
       (back-to-indentation)
       (clean-aindent--bsunindent 1)
       (let ((one (list :text (buffer-string)
                        :point (point)
                        :column (current-column)
                        :indent (current-indentation))))
         (clean-aindent--bsunindent 1)
         (list :one one
               :two (list :text (buffer-string)
                          :point (point)
                          :column (current-column)
                          :indent (current-indentation))))))))
"####;
    let expected = expect![[
        r#"OK (:one (:text "root\n\11child\n\11deploy-production" :point 14 :column 4 :indent 4) :two (:text "root\n\11child\ndeploy-production" :point 13 :column 0 :indent 0))"#
    ]];
    ParityBatchCase::value(
        "tabbed_indentation_is_removed_by_visual_column_without_damaging_text",
        elisp_form,
        expected,
    )
}

fn backspace_unindent_outside_leading_space_preserves_backward_kill_word_semantics()
-> ParityBatchCase {
    let elisp_form = r####"
(neomacs-clean-aindent-test-with-mode
 nil
 (lambda ()
   (with-temp-buffer
     (let ((kill-ring nil)
           (kill-ring-yank-pointer nil))
       (insert "deploy staging production")
       (clean-aindent--bsunindent 2)
       (list :text (buffer-string)
             :point (point)
             :column (current-column)
             :kill-ring (copy-tree kill-ring)
             :last-command last-command
             :this-command this-command)))))
"####;
    let expected = expect![[
        r#"OK (:text "deploy " :point 8 :column 7 :kill-ring ("staging production") :last-command nil :this-command kill-region)"#
    ]];
    ParityBatchCase::value(
        "backspace_unindent_outside_leading_space_preserves_backward_kill_word_semantics",
        elisp_form,
        expected,
    )
}

fn tracked_auto_indents_are_buffer_local_and_cleanup_cannot_cross_buffers() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-clean-aindent-test-with-mode
 nil
 (lambda ()
   (let ((alpha (generate-new-buffer " *clean-aindent-alpha*"))
         (beta (generate-new-buffer " *clean-aindent-beta*")))
     (unwind-protect
         (progn
           (with-current-buffer alpha
             (emacs-lisp-mode)
             (insert "(let ((alpha 1))")
             (newline-and-indent))
           (with-current-buffer beta
             (emacs-lisp-mode)
             (insert "(let ((beta 2))")
             (newline-and-indent))
           (let ((initial
                  (list
                   :alpha (with-current-buffer alpha
                            (neomacs-clean-aindent-test-state))
                   :beta (with-current-buffer beta
                           (neomacs-clean-aindent-test-state)))))
             (with-current-buffer alpha
               (goto-char (point-min))
               (run-hooks 'post-command-hook))
             (let ((after-alpha
                    (list
                     :alpha (with-current-buffer alpha
                              (neomacs-clean-aindent-test-state))
                     :beta (with-current-buffer beta
                             (neomacs-clean-aindent-test-state)))))
               (with-current-buffer beta
                 (goto-char (point-min))
                 (run-hooks 'post-command-hook))
               (list :initial initial
                     :after-alpha after-alpha
                     :after-both
                     (list
                      :alpha (with-current-buffer alpha
                               (neomacs-clean-aindent-test-state))
                      :beta (with-current-buffer beta
                              (neomacs-clean-aindent-test-state)))))))
       (kill-buffer alpha)
       (kill-buffer beta)))))
"####;
    let expected = expect![[
        r#"OK (:initial (:alpha (:text "(let ((alpha 1))\n  " :point 20 :column 2 :tracked 20 :tracked-length 2 :tracked-local t :length-local t) :beta (:text "(let ((beta 2))\n  " :point 19 :column 2 :tracked 19 :tracked-length 2 :tracked-local t :length-local t)) :after-alpha (:alpha (:text "(let ((alpha 1))\n" :point 1 :column 0 :tracked nil :tracked-length 2 :tracked-local t :length-local t) :beta (:text "(let ((beta 2))\n  " :point 19 :column 2 :tracked 19 :tracked-length 2 :tracked-local t :length-local t)) :after-both (:alpha (:text "(let ((alpha 1))\n" :point 1 :column 0 :tracked nil :tracked-length 2 :tracked-local t :length-local t) :beta (:text "(let ((beta 2))\n" :point 1 :column 0 :tracked nil :tracked-length 2 :tracked-local t :length-local t)))"#
    ]];
    ParityBatchCase::value(
        "tracked_auto_indents_are_buffer_local_and_cleanup_cannot_cross_buffers",
        elisp_form,
        expected,
    )
}

fn global_mode_lifecycle_installs_and_removes_advice_hook_and_command_remapping() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((descriptor (cadr (assq 'clean-aindent-mode package-alist)))
       (before (list :mode clean-aindent-mode
                     :advice-active (and (ad-is-active 'newline-and-indent) t)
                     :hook (and (memq 'clean-aindent--check-last-point
                                      post-command-hook)
                                t))))
  (unwind-protect
      (progn
        (clean-aindent-mode 1)
        (let ((enabled
               (with-temp-buffer
                 (list :mode clean-aindent-mode
                       :advice-active (and (ad-is-active 'newline-and-indent) t)
                       :hook (and (memq 'clean-aindent--check-last-point
                                        post-command-hook)
                                  t)
                       :remap (command-remapping 'backward-kill-word)))))
          (clean-aindent-mode -1)
          (list :package (package-desc-name descriptor)
                :version (package-version-join
                          (package-desc-version descriptor))
                :requirements (package-desc-reqs descriptor)
                :feature (featurep 'clean-aindent-mode)
                :mode-command (commandp 'clean-aindent-mode)
                :custom-type (get 'clean-aindent-is-simple-indent
                                  'custom-type)
                :keymap-remap
                (lookup-key clean-aindent-mode--keymap
                            [remap backward-kill-word])
                :before before
                :enabled enabled
                :disabled
                (list :mode clean-aindent-mode
                      :advice-active
                      (and (ad-is-active 'newline-and-indent) t)
                      :hook
                      (and (memq 'clean-aindent--check-last-point
                                 post-command-hook)
                           t)
                      :remap
                      (with-temp-buffer
                        (command-remapping 'backward-kill-word))))))
    (clean-aindent-mode -1)))
"####;
    let expected = expect![[
        r#"OK (:package clean-aindent-mode :version "20171017.2043" :requirements nil :feature t :mode-command t :custom-type boolean :keymap-remap clean-aindent--bsunindent :before (:mode nil :advice-active nil :hook nil) :enabled (:mode t :advice-active t :hook t :remap clean-aindent--bsunindent) :disabled (:mode nil :advice-active nil :hook nil :remap nil))"#
    ]];
    ParityBatchCase::value(
        "global_mode_lifecycle_installs_and_removes_advice_hook_and_command_remapping",
        elisp_form,
        expected,
    )
}

#[test]
fn clean_aindent_mode_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(CLEAN_AINDENT_MODE_MELPA_PIN, "clean-aindent-mode.el")
            .expect("prepare revision-pinned Clean Aindent Mode source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "clean-aindent-mode-package-batch",
        "Clean Aindent Mode",
        &[
            smart_indent_trims_an_abandoned_language_indent_after_the_next_command(),
            simple_indent_cleans_trailing_space_and_reuses_the_nearest_nonblank_indent(),
            entering_real_code_preserves_the_indented_line_when_point_moves_away(),
            abandoned_indent_cleanup_can_be_undone_without_losing_typed_code(),
            repeated_backspace_unindent_walks_real_nested_source_to_each_outer_level(),
            tabbed_indentation_is_removed_by_visual_column_without_damaging_text(),
            backspace_unindent_outside_leading_space_preserves_backward_kill_word_semantics(),
            tracked_auto_indents_are_buffer_local_and_cleanup_cannot_cross_buffers(),
            global_mode_lifecycle_installs_and_removes_advice_hook_and_command_remapping(),
        ],
    );
}
