use expect_test::expect;

use super::ParityBatchCase;

fn atom_dark_theme_mode_branch_matrix_calls_face_remapping_with_exact_face_and_recipe()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_mode_branch_matrix_calls_face_remapping_with_exact_face_and_recipe",
        r##"(let (observations)
         (dolist
             (mode
              '(conf-mode
                conf-javaprop-mode
                html-mode
                yaml-mode
                java-mode
                markdown-mode
                javascript-mode
                js2-mode
                text-mode
                fundamental-mode))
           (let ((major-mode mode)
                 calls)
             (cl-letf
                 (((symbol-function
                    'face-remap-add-relative)
                   (lambda
                     (face &rest specs)
                     (push
                      (cons face specs)
                      calls)
                     (list :cookie face specs))))
               (push
                (list
                 mode
                 (atom-dark-theme-change-faces-for-mode)
                 (nreverse calls))
                observations))))
         (nreverse observations))"##,
        expect![[
            r##"OK ((conf-mode (:cookie font-lock-variable-name-face #1=(#2=(:inherit (font-lock-keyword-face)))) ((font-lock-variable-name-face . #1#))) (conf-javaprop-mode (:cookie font-lock-variable-name-face #3=(#2#)) ((font-lock-variable-name-face . #3#))) (html-mode (:cookie font-lock-variable-name-face #4=(#2#)) ((font-lock-variable-name-face . #4#))) (yaml-mode (:cookie font-lock-variable-name-face #5=(#2#)) ((font-lock-variable-name-face . #5#))) (java-mode (:cookie font-lock-variable-name-face #6=((:inherit (js2-function-param)))) ((font-lock-variable-name-face . #6#))) (markdown-mode (:cookie default #7=((:foreground "#999"))) ((default . #7#))) (javascript-mode (:cookie font-lock-doc-face #8=(#9=(:inherit (font-lock-comment-face)))) ((font-lock-doc-face . #8#))) (js2-mode (:cookie font-lock-doc-face #10=(#9#)) ((font-lock-doc-face . #10#))) (text-mode nil nil) (fundamental-mode nil nil))"##
        ]],
    )
    .fresh_process()
}

fn atom_dark_theme_actual_buffer_local_remapping_alists_match_for_every_supported_mode()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_actual_buffer_local_remapping_alists_match_for_every_supported_mode",
        r##"(mapcar
         (lambda (mode)
           (with-temp-buffer
             (setq major-mode mode)
             (let ((result
                    (atom-dark-theme-change-faces-for-mode)))
               (list
                mode
                result
                face-remapping-alist
                (local-variable-p
                 'face-remapping-alist)))))
         '(conf-mode
           conf-javaprop-mode
           html-mode
           yaml-mode
           java-mode
           markdown-mode
           javascript-mode
           js2-mode
           text-mode
           fundamental-mode))"##,
        expect![[
            r##"OK ((conf-mode (font-lock-variable-name-face . #1=(:inherit (font-lock-keyword-face))) ((font-lock-variable-name-face #1# font-lock-variable-name-face)) t) (conf-javaprop-mode (font-lock-variable-name-face . #1#) ((font-lock-variable-name-face #1# font-lock-variable-name-face)) t) (html-mode (font-lock-variable-name-face . #1#) ((font-lock-variable-name-face #1# font-lock-variable-name-face)) t) (yaml-mode (font-lock-variable-name-face . #1#) ((font-lock-variable-name-face #1# font-lock-variable-name-face)) t) (java-mode (font-lock-variable-name-face . #2=(:inherit (js2-function-param))) ((font-lock-variable-name-face #2# font-lock-variable-name-face)) t) (markdown-mode (default . #3=(:foreground "#999")) ((default #3# default)) t) (javascript-mode (font-lock-doc-face . #4=(:inherit (font-lock-comment-face))) ((font-lock-doc-face #4# font-lock-doc-face)) t) (js2-mode (font-lock-doc-face . #4#) ((font-lock-doc-face #4# font-lock-doc-face)) t) (text-mode nil nil nil) (fundamental-mode nil nil nil))"##
        ]],
    )
    .fresh_process()
}

fn atom_dark_theme_force_switch_accepts_only_exact_t_and_leaves_other_truthy_values_inert()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_force_switch_accepts_only_exact_t_and_leaves_other_truthy_values_inert",
        r##"(mapcar
         (lambda (value)
           (with-temp-buffer
             (setq major-mode 'html-mode)
             (let ((atom-dark-theme-force-faces-for-mode
                    value))
               (list
                value
                (atom-dark-theme-change-faces-for-mode)
                face-remapping-alist
                (local-variable-p
                 'face-remapping-alist)))))
         '(t nil 1 enabled "t" (t)))"##,
        expect![[
            r#"OK ((t (font-lock-variable-name-face . #1=(:inherit (font-lock-keyword-face))) ((font-lock-variable-name-face #1# font-lock-variable-name-face)) t) (nil nil nil nil) (1 nil nil nil) (enabled nil nil nil) ("t" nil nil nil) ((t) nil nil nil))"#
        ]],
    )
}

fn atom_dark_theme_command_interactive_contract_returns_cookie_and_mutates_only_current_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_command_interactive_contract_returns_cookie_and_mutates_only_current_buffer",
        r##"(let ((outside face-remapping-alist)
               inside)
         (with-temp-buffer
           (setq major-mode 'markdown-mode)
           (setq inside
                 (list
                  (commandp
                   'atom-dark-theme-change-faces-for-mode)
                  (interactive-form
                   'atom-dark-theme-change-faces-for-mode)
                  (call-interactively
                   #'atom-dark-theme-change-faces-for-mode)
                  face-remapping-alist
                  (local-variable-p
                   'face-remapping-alist))))
         (list
          inside
          face-remapping-alist
          (equal outside face-remapping-alist)))"##,
        expect![[
            r##"OK ((t (interactive nil) (default . #1=(:foreground "#999")) ((default #1# default)) t) nil t)"##
        ]],
    )
    .fresh_process()
}

fn atom_dark_theme_registered_hook_runs_once_and_applies_a_real_html_mode_transition()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_registered_hook_runs_once_and_applies_a_real_html_mode_transition",
        r##"(let ((hook-count 0))
         (dolist
             (function after-change-major-mode-hook)
           (when
               (eq
                function
                'atom-dark-theme-change-faces-for-mode)
             (setq hook-count
                   (1+ hook-count))))
         (with-temp-buffer
           (insert
            "<div class=\"card\" data-kind=\"primary\">Hello</div>")
           (html-mode)
           (font-lock-ensure)
           (list
            hook-count
            major-mode
            face-remapping-alist
            (local-variable-p
             'face-remapping-alist)
            (get-text-property
             (point-min)
             'face)
            (mapcar
             (lambda (token)
               (goto-char
                (point-min))
               (search-forward token)
               (list
                token
                (get-text-property
                 (match-beginning 0)
                 'face)))
             '("div"
               "class"
               "\"card\""
               "data-kind"
               "\"primary\""
               "Hello"))
            (buffer-substring-no-properties
             (point-min)
             (point-max)))))"##,
        expect![[
            r#"OK (1 html-mode ((font-lock-variable-name-face (:inherit (font-lock-keyword-face)) font-lock-variable-name-face)) t nil (("div" font-lock-function-name-face) ("class" font-lock-variable-name-face) ("\"card\"" font-lock-string-face) ("data-kind" font-lock-variable-name-face) ("\"primary\"" font-lock-string-face) ("Hello" nil)) "<div class=\"card\" data-kind=\"primary\">Hello</div>")"#
        ]],
    )
    .fresh_process()
}

fn atom_dark_theme_major_mode_changes_clear_stale_remaps_and_hook_applies_new_mode_recipe()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_major_mode_changes_clear_stale_remaps_and_hook_applies_new_mode_recipe",
        r##"(with-temp-buffer
         (setq major-mode 'html-mode)
         (run-hooks 'after-change-major-mode-hook)
         (let ((html
                (copy-tree face-remapping-alist)))
           (kill-all-local-variables)
           (setq major-mode 'markdown-mode)
           (run-hooks 'after-change-major-mode-hook)
           (let ((markdown
                  (copy-tree face-remapping-alist)))
             (kill-all-local-variables)
             (setq major-mode 'text-mode)
             (run-hooks 'after-change-major-mode-hook)
             (list
              html
              markdown
              face-remapping-alist
              (local-variable-p
               'face-remapping-alist)))))"##,
        expect![[
            r##"OK (((font-lock-variable-name-face (:inherit (font-lock-keyword-face)) font-lock-variable-name-face)) ((default (:foreground "#999") default)) nil nil)"##
        ]],
    )
    .fresh_process()
}

fn atom_dark_theme_repeated_remapping_calls_coalesce_and_cookie_removal_matches_gnu()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_repeated_remapping_calls_coalesce_and_cookie_removal_matches_gnu",
        r##"(with-temp-buffer
         (setq major-mode 'java-mode)
         (let ((first
                (atom-dark-theme-change-faces-for-mode))
               second
               after-both
               after-first-removal)
           (setq second
                 (atom-dark-theme-change-faces-for-mode))
           (setq after-both
                 (copy-tree face-remapping-alist))
           (face-remap-remove-relative first)
           (setq after-first-removal
                 (copy-tree face-remapping-alist))
           (face-remap-remove-relative second)
           (list
            first
            second
            (equal first second)
            after-both
            (length
             (cdr
              (assq
               'font-lock-variable-name-face
               after-both)))
            after-first-removal
            face-remapping-alist)))"##,
        expect![
            "OK ((font-lock-variable-name-face . #1=(:inherit (js2-function-param))) (font-lock-variable-name-face . #1#) t ((font-lock-variable-name-face (:inherit (js2-function-param)) (:inherit (js2-function-param)) font-lock-variable-name-face)) 3 nil nil)"
        ],
    )
    .fresh_process()
}

fn atom_dark_theme_manual_hook_execution_obeys_dynamic_force_disable_and_reenable()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_manual_hook_execution_obeys_dynamic_force_disable_and_reenable",
        r##"(with-temp-buffer
         (setq major-mode 'javascript-mode)
         (let ((atom-dark-theme-force-faces-for-mode nil))
           (run-hooks 'after-change-major-mode-hook))
         (let ((disabled
                (list
                 face-remapping-alist
                 (local-variable-p
                  'face-remapping-alist))))
           (let ((atom-dark-theme-force-faces-for-mode t))
             (run-hooks 'after-change-major-mode-hook))
           (list
            disabled
            face-remapping-alist
            (local-variable-p
             'face-remapping-alist))))"##,
        expect![
            "OK ((nil nil) ((font-lock-doc-face (:inherit (font-lock-comment-face)) font-lock-doc-face)) t)"
        ],
    )
}

fn atom_dark_theme_remapping_errors_propagate_without_falling_through_to_other_mode_branches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_remapping_errors_propagate_without_falling_through_to_other_mode_branches",
        r##"(mapcar
         (lambda (mode)
           (let ((major-mode mode)
                 calls)
             (cl-letf
                 (((symbol-function
                    'face-remap-add-relative)
                   (lambda
                     (face &rest specs)
                     (push
                      (cons face specs)
                      calls)
                     (error
                      "fixture remap failure for %S"
                      face))))
               (list
                mode
                (atom-dark-test-error
                 (lambda ()
                   (atom-dark-theme-change-faces-for-mode)))
                (nreverse calls)))))
         '(html-mode
           java-mode
           markdown-mode
           js2-mode
           text-mode))"##,
        expect![[
            r##"OK ((html-mode (:signal error ("fixture remap failure for font-lock-variable-name-face")) ((font-lock-variable-name-face (:inherit (font-lock-keyword-face))))) (java-mode (:signal error ("fixture remap failure for font-lock-variable-name-face")) ((font-lock-variable-name-face (:inherit (js2-function-param))))) (markdown-mode (:signal error ("fixture remap failure for default")) ((default (:foreground "#999")))) (js2-mode (:signal error ("fixture remap failure for font-lock-doc-face")) ((font-lock-doc-face (:inherit (font-lock-comment-face))))) (text-mode (:ok nil) nil))"##
        ]],
    )
    .fresh_process()
}

fn atom_dark_theme_unsupported_modes_and_disabled_force_return_nil_without_local_side_effects()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_dark_theme_unsupported_modes_and_disabled_force_return_nil_without_local_side_effects",
        r##"(list
         (with-temp-buffer
           (setq major-mode 'special-mode)
           (list
            (atom-dark-theme-change-faces-for-mode)
            face-remapping-alist
            (local-variable-p
             'face-remapping-alist)))
         (with-temp-buffer
           (setq major-mode 'html-mode)
           (setq-local
            atom-dark-theme-force-faces-for-mode
            nil)
           (list
            (atom-dark-theme-change-faces-for-mode)
            face-remapping-alist
            (local-variable-p
             'face-remapping-alist)
            (local-variable-p
             'atom-dark-theme-force-faces-for-mode))))"##,
        expect!["OK ((nil nil nil) (nil nil nil t))"],
    )
}

pub(super) fn remapping_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atom_dark_theme_mode_branch_matrix_calls_face_remapping_with_exact_face_and_recipe(),
        atom_dark_theme_actual_buffer_local_remapping_alists_match_for_every_supported_mode(),
        atom_dark_theme_force_switch_accepts_only_exact_t_and_leaves_other_truthy_values_inert(),
        atom_dark_theme_command_interactive_contract_returns_cookie_and_mutates_only_current_buffer(
        ),
        atom_dark_theme_registered_hook_runs_once_and_applies_a_real_html_mode_transition(),
        atom_dark_theme_major_mode_changes_clear_stale_remaps_and_hook_applies_new_mode_recipe(),
        atom_dark_theme_repeated_remapping_calls_coalesce_and_cookie_removal_matches_gnu(),
        atom_dark_theme_manual_hook_execution_obeys_dynamic_force_disable_and_reenable(),
        atom_dark_theme_remapping_errors_propagate_without_falling_through_to_other_mode_branches(),
        atom_dark_theme_unsupported_modes_and_disabled_force_return_nil_without_local_side_effects(
        ),
    ]
}
