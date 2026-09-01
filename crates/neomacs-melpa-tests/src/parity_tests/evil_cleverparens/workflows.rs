use expect_test::expect;

use super::ParityBatchCase;

fn normal_x_splices_a_nested_form_without_unbalancing_the_program() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (let ((buffer (generate-new-buffer " *evil-cleverparens deployment*"))
        (evil-surround-mode-hook nil)
        (kill-ring '("previous-operation"))
        (kill-ring-yank-pointer nil))
    (unwind-protect
        (progn
          (switch-to-buffer buffer)
          (emacs-lisp-mode)
          (insert "(deploy (validate release) (publish release))")
          (goto-char (point-min))
          (search-forward "(validate")
          (goto-char (match-beginning 0))
          (evil-local-mode 1)
          (evil-cleverparens-mode 1)
          (evil-normal-state)
          (execute-kbd-macro (kbd "x"))
          (list :state (neomacs-evil-cleverparens-test-state)
                :x-binding (key-binding (kbd "x"))
                :mode-lighter
                (cdr (assq 'evil-cleverparens-mode minor-mode-alist))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (evil-cleverparens-mode -1)
          (evil-local-mode -1))
        (kill-buffer buffer)))))
"##;
    let expected = expect![[
        r#"OK (:state (:buffer "(deploy validate release (publish release))" :point 9 :line 1 :column 8 :char 118 :evil-state normal :balanced t :kill "(") :x-binding evil-cp-delete-char-or-splice :mode-lighter ((" ecp" (:eval (if evil-cleverparens-complete-parens-in-yanked-region "/b" "/i")))))"#
    ]];
    ParityBatchCase::value(
        "normal_x_splices_a_nested_form_without_unbalancing_the_program",
        elisp_form,
        expected,
    )
}

fn normal_dd_removes_an_unbalanced_workflow_line_but_preserves_its_structure() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (let ((buffer (generate-new-buffer " *evil-cleverparens line delete*"))
        (evil-surround-mode-hook nil)
        (kill-ring '("previous-operation"))
        (kill-ring-yank-pointer nil))
    (unwind-protect
        (progn
          (switch-to-buffer buffer)
          (emacs-lisp-mode)
          (insert "(deploy\n"
                  "  (when release-ready\n"
                  "    (validate release)\n"
                  "    (publish release))\n"
                  "  (notify release))\n")
          (goto-char (point-min))
          (search-forward "release-ready")
          (evil-local-mode 1)
          (evil-cleverparens-mode 1)
          (evil-normal-state)
          (execute-kbd-macro (kbd "dd"))
          (list :state (neomacs-evil-cleverparens-test-state)
                :unnamed-register
                (substring-no-properties (evil-get-register ?\"))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (evil-cleverparens-mode -1)
          (evil-local-mode -1))
        (kill-buffer buffer)))))
"##;
    let expected = expect![[
        r#"OK (:state (:buffer "(deploy\n ((validate release)\n  (publish release))\n (notify release))\n" :point 28 :line 2 :column 19 :char 41 :evil-state normal :balanced t :kill "when release-ready\n") :unnamed-register "when release-ready\n")"#
    ]];
    ParityBatchCase::value(
        "normal_dd_removes_an_unbalanced_workflow_line_but_preserves_its_structure",
        elisp_form,
        expected,
    )
}

fn slurp_then_drag_restructures_a_release_pipeline_through_real_keys() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (let ((buffer (generate-new-buffer " *evil-cleverparens pipeline*"))
        (evil-surround-mode-hook nil))
    (unwind-protect
        (progn
          (switch-to-buffer buffer)
          (emacs-lisp-mode)
          (insert "(pipeline (validate release) (publish release)) (notify release)")
          (goto-char (point-min))
          (search-forward "validate")
          (evil-local-mode 1)
          (evil-cleverparens-mode 1)
          (evil-normal-state)
          (execute-kbd-macro (kbd ">"))
          (let ((after-slurp (buffer-string)))
            (goto-char (point-min))
            (search-forward "publish")
            (backward-char)
            (execute-kbd-macro (kbd "M-k"))
            (list :after-slurp after-slurp
                  :after-drag (neomacs-evil-cleverparens-test-state))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (evil-cleverparens-mode -1)
          (evil-local-mode -1))
        (kill-buffer buffer)))))
"##;
    let expected = expect![[
        r#"OK (:after-slurp "(pipeline (validate release (publish release))) (notify release)" :after-drag (:buffer "(pipeline (validate (publish release) release)) (notify release)" :point 28 :line 1 :column 27 :char 104 :evil-state normal :balanced t :kill nil))"#
    ]];
    ParityBatchCase::value(
        "slurp_then_drag_restructures_a_release_pipeline_through_real_keys",
        elisp_form,
        expected,
    )
}

fn form_defun_and_comment_text_objects_select_practical_editing_scopes() -> ParityBatchCase {
    let elisp_form = r##"
(let ((evil-surround-mode-hook nil))
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert ";; Publish a validated release.\n"
            "(defun deploy-release (release)\n"
            "  (when (validate release)\n"
            "    (publish release)))\n")
    (evil-cleverparens-mode 1)
    (goto-char (point-min))
    (search-forward "validate release")
    (let ((outer-form
           (neomacs-evil-cleverparens-test-range (evil-cp-a-form 1)))
          (inner-form
           (neomacs-evil-cleverparens-test-range (evil-cp-inner-form 1)))
          (defun
           (neomacs-evil-cleverparens-test-range (evil-cp-a-defun 1))))
      (goto-char (point-min))
      (search-forward "validated")
      (list :outer-form outer-form
            :inner-form inner-form
            :defun defun
            :comment
            (neomacs-evil-cleverparens-test-range (evil-cp-a-comment 1))))))
"##;
    let expected = expect![[
        r#"OK (:outer-form (:begin 73 :end 91 :type inclusive :text "(validate release)") :inner-form (:begin 74 :end 90 :type inclusive :text "validate release") :defun (:begin 33 :end 115 :type inclusive :text "(defun deploy-release (release)\n  (when (validate release)\n    (publish release)))") :comment (:begin 1 :end 32 :type exclusive :text ";; Publish a validated release."))"#
    ]];
    ParityBatchCase::value(
        "form_defun_and_comment_text_objects_select_practical_editing_scopes",
        elisp_form,
        expected,
    )
}

fn balanced_yank_policy_switches_between_reusable_forms_and_literal_source() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (let ((buffer (generate-new-buffer " *evil-cleverparens yank policy*"))
        (evil-surround-mode-hook nil)
        (kill-ring '("previous-operation"))
        (kill-ring-yank-pointer nil)
        (evil-cleverparens-complete-parens-in-yanked-region t))
    (unwind-protect
        (progn
          (switch-to-buffer buffer)
          (emacs-lisp-mode)
          (insert "(deploy (when release-ready\n"
                  "          (publish release)))")
          (goto-char (point-min))
          (search-forward "when")
          (evil-local-mode 1)
          (evil-cleverparens-mode 1)
          (evil-normal-state)
          (execute-kbd-macro (kbd "yy"))
          (let ((balanced
                 (substring-no-properties (evil-get-register ?\"))))
            (evil-cp-toggle-balanced-yank)
            (execute-kbd-macro (kbd "yy"))
            (let ((literal
                   (substring-no-properties (evil-get-register ?\"))))
              (evil-cp-toggle-balanced-yank 'force)
              (list :balanced balanced
                    :literal literal
                    :restored-policy
                    evil-cleverparens-complete-parens-in-yanked-region
                    :buffer (buffer-string)
                    :point (point)
                    :state evil-state))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (evil-cleverparens-mode -1)
          (evil-local-mode -1))
        (kill-buffer buffer)))))
"##;
    let expected = expect![[
        r#"OK (:balanced "(deploy (when release-ready))" :literal "when release-ready\n" :restored-policy t :buffer "(deploy (when release-ready\n          (publish release)))" :point 14 :state normal)"#
    ]];
    ParityBatchCase::value(
        "balanced_yank_policy_switches_between_reusable_forms_and_literal_source",
        elisp_form,
        expected,
    )
}

fn mode_lifecycle_and_append_key_support_a_real_guard_insertion() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (let ((buffer (generate-new-buffer " *evil-cleverparens guard insertion*"))
        (evil-surround-mode-hook nil)
        events)
    (unwind-protect
        (progn
          (switch-to-buffer buffer)
          (emacs-lisp-mode)
          (insert "(deploy (publish release))")
          (goto-char (point-min))
          (search-forward "(publish")
          (goto-char (match-beginning 0))
          (evil-local-mode 1)
          (let ((evil-cleverparens-enabled-hook
                 (list (lambda () (push :enabled events))))
                (evil-cleverparens-disabled-hook
                 (list (lambda () (push :disabled events)))))
            (evil-cleverparens-mode 1)
            (evil-normal-state)
            (let ((bindings
                   (list :append (key-binding (kbd "a"))
                         :delete (key-binding (kbd "d"))
                         :open-form (key-binding (kbd "M-o"))
                         :inner-form
                         (lookup-key evil-inner-text-objects-map "f"))))
              (execute-kbd-macro (kbd "a"))
              (execute-kbd-macro "when-ready")
              (execute-kbd-macro [escape])
              (evil-cleverparens-mode -1)
              (list :source (buffer-string)
                    :point (point)
                    :state evil-state
                    :bindings bindings
                    :events (nreverse events)
                    :mode evil-cleverparens-mode))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (evil-cleverparens-mode -1)
          (evil-local-mode -1))
        (kill-buffer buffer)))))
"##;
    let expected = expect![[
        r#"OK (:source "(deploy (when-ready publish release))" :point 19 :state normal :bindings (:append evil-cp-append :delete evil-cp-delete :open-form evil-cp-open-below-form :inner-form evil-cp-inner-form) :events (:enabled :disabled) :mode nil)"#
    ]];
    ParityBatchCase::value(
        "mode_lifecycle_and_append_key_support_a_real_guard_insertion",
        elisp_form,
        expected,
    )
}

fn copy_raise_and_wrap_commands_transform_real_pipeline_forms() -> ParityBatchCase {
    let elisp_form = r##"
(save-window-excursion
  (let ((buffer (generate-new-buffer " *evil-cleverparens form transforms*"))
        (evil-surround-mode-hook nil))
    (unwind-protect
        (progn
          (switch-to-buffer buffer)
          (emacs-lisp-mode)
          (insert "(deploy (validate release) (publish release) (notify release))")
          (goto-char (point-min))
          (search-forward "publish")
          (evil-local-mode 1)
          (evil-cleverparens-mode 1)
          (evil-normal-state)
          (execute-kbd-macro (kbd "M-w"))
          (let ((after-copy (buffer-string)))
            (execute-kbd-macro (kbd "M-R"))
            (let ((after-raise (buffer-string)))
              (forward-char)
              (execute-kbd-macro (kbd "M-("))
              (list :after-copy after-copy
                    :after-raise after-raise
                    :after-wrap
                    (neomacs-evil-cleverparens-test-state)))))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (evil-cleverparens-mode -1)
          (evil-local-mode -1))
        (kill-buffer buffer)))))
"##;
    let expected = expect![[
        r#"OK (:after-copy "(deploy (validate release) (publish release)\n\11(publish release) (notify release))" :after-raise "(publish release)" :after-wrap (:buffer "(publish (release))" :point 11 :line 1 :column 10 :char 114 :evil-state normal :balanced t :kill nil))"#
    ]];
    ParityBatchCase::value(
        "copy_raise_and_wrap_commands_transform_real_pipeline_forms",
        elisp_form,
        expected,
    )
}

fn structural_motions_navigate_nested_forms_and_neighboring_definitions() -> ParityBatchCase {
    let elisp_form = r##"
(let ((evil-surround-mode-hook nil))
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "(defun deploy (release)\n"
            "  (when (validate release)\n"
            "    (publish release)))\n\n"
            "(defun rollback (release)\n"
            "  (restore release))\n")
    (evil-cleverparens-mode 1)
    (goto-char (point-min))
    (search-forward "publish")
    (let (up-opening up-closing defun-start defun-end next-defun)
      (evil-cp-backward-up-sexp 1)
      (setq up-opening
            (list :point (point) :char (char-after)
                  :line (line-number-at-pos)))
      (forward-char)
      (evil-cp-up-sexp 1)
      (setq up-closing
            (list :point (point) :char (char-after)
                  :line (line-number-at-pos)))
      (evil-cp-beginning-of-defun 1)
      (setq defun-start
            (list :point (point) :char (char-after)
                  :line (line-number-at-pos)))
      (evil-cp-end-of-defun 1)
      (setq defun-end
            (list :point (point) :char (char-after)
                  :line (line-number-at-pos)))
      (forward-char)
      (skip-chars-forward " \n\t")
      (evil-cp-forward-sexp 1)
      (setq next-defun
            (list :point (point) :before (char-before)
                  :line (line-number-at-pos)))
      (list :up-opening up-opening
            :up-closing up-closing
            :defun-start defun-start
            :defun-end defun-end
            :next-defun next-defun))))
"##;
    let expected = expect![[
        r#"OK (:up-opening (:point 56 :char 40 :line 3) :up-closing (:point 72 :char 41 :line 3) :defun-start (:point 1 :char 40 :line 1) :defun-end (:point 74 :char 41 :line 3) :next-defun (:point 123 :before 41 :line 6))"#
    ]];
    ParityBatchCase::value(
        "structural_motions_navigate_nested_forms_and_neighboring_definitions",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        normal_x_splices_a_nested_form_without_unbalancing_the_program(),
        normal_dd_removes_an_unbalanced_workflow_line_but_preserves_its_structure(),
        slurp_then_drag_restructures_a_release_pipeline_through_real_keys(),
        form_defun_and_comment_text_objects_select_practical_editing_scopes(),
        balanced_yank_policy_switches_between_reusable_forms_and_literal_source(),
        mode_lifecycle_and_append_key_support_a_real_guard_insertion(),
        copy_raise_and_wrap_commands_transform_real_pipeline_forms(),
        structural_motions_navigate_nested_forms_and_neighboring_definitions(),
    ]
}
