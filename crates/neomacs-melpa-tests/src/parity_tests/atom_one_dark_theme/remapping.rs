use expect_test::expect;

use super::ParityBatchCase;

fn atom_one_dark_theme_mode_branch_matrix_calls_exact_faces_colors_and_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_mode_branch_matrix_calls_exact_faces_colors_and_order",
        r##"(let (observations)
         (dolist
             (mode
              '(js2-mode
                html-mode
                javascript-mode
                web-mode
                text-mode))
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
                     (list
                      :cookie
                      face
                      specs))))
               (push
                (list
                 mode
                 (atom-one-dark-theme-change-faces-for-mode)
                 (nreverse calls))
                observations))))
         (nreverse observations))"##,
        expect![[
            r##"OK ((js2-mode (:cookie font-lock-variable-name-face #1=(:foreground "#ABB2BF")) ((font-lock-constant-face :foreground "#D19A66") (font-lock-doc-face (:inherit (font-lock-comment-face))) (font-lock-variable-name-face . #1#))) (html-mode (:cookie font-lock-variable-name-face #2=(:foreground "#D19A66")) ((font-lock-function-name-face :foreground "#E06C75") (font-lock-variable-name-face . #2#))) (javascript-mode nil nil) (web-mode nil nil) (text-mode nil nil))"##
        ]],
    )
    .fresh_process()
}

fn atom_one_dark_theme_actual_buffer_local_remapping_alists_match_supported_modes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_actual_buffer_local_remapping_alists_match_supported_modes",
        r##"(mapcar
         (lambda (mode)
           (with-temp-buffer
             (setq major-mode mode)
             (let ((result
                    (atom-one-dark-theme-change-faces-for-mode)))
               (list
                mode
                result
                face-remapping-alist
                (local-variable-p
                 'face-remapping-alist)))))
         '(js2-mode
           html-mode
           javascript-mode
           web-mode
           text-mode))"##,
        expect![[
            r##"OK ((js2-mode (font-lock-variable-name-face . #1=(:foreground "#ABB2BF")) ((font-lock-variable-name-face #1# font-lock-variable-name-face) (font-lock-doc-face (:inherit (font-lock-comment-face)) font-lock-doc-face) (font-lock-constant-face (:foreground "#D19A66") font-lock-constant-face)) t) (html-mode (font-lock-variable-name-face . #2=(:foreground "#D19A66")) ((font-lock-variable-name-face #2# font-lock-variable-name-face) (font-lock-function-name-face (:foreground "#E06C75") font-lock-function-name-face)) t) (javascript-mode nil nil nil) (web-mode nil nil nil) (text-mode nil nil nil))"##
        ]],
    )
    .fresh_process()
}

fn atom_one_dark_theme_force_gate_accepts_every_truthy_value_but_rejects_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_force_gate_accepts_every_truthy_value_but_rejects_nil",
        r##"(mapcar
         (lambda (value)
           (with-temp-buffer
             (setq major-mode 'html-mode)
             (let ((atom-one-dark-theme-force-faces-for-mode
                    value))
               (list
                value
                (atom-one-dark-theme-change-faces-for-mode)
                face-remapping-alist
                (local-variable-p
                 'face-remapping-alist)))))
         '(t nil 0 enabled "yes" (enabled)))"##,
        expect![[
            r##"OK ((t (font-lock-variable-name-face . #1=(:foreground "#D19A66")) ((font-lock-variable-name-face #1# font-lock-variable-name-face) (font-lock-function-name-face (:foreground "#E06C75") font-lock-function-name-face)) t) (nil nil nil nil) (0 (font-lock-variable-name-face . #2=(:foreground "#D19A66")) ((font-lock-variable-name-face #2# font-lock-variable-name-face) (font-lock-function-name-face (:foreground "#E06C75") font-lock-function-name-face)) t) (enabled (font-lock-variable-name-face . #3=(:foreground "#D19A66")) ((font-lock-variable-name-face #3# font-lock-variable-name-face) (font-lock-function-name-face (:foreground "#E06C75") font-lock-function-name-face)) t) ("yes" (font-lock-variable-name-face . #4=(:foreground "#D19A66")) ((font-lock-variable-name-face #4# font-lock-variable-name-face) (font-lock-function-name-face (:foreground "#E06C75") font-lock-function-name-face)) t) ((enabled) (font-lock-variable-name-face . #5=(:foreground "#D19A66")) ((font-lock-variable-name-face #5# font-lock-variable-name-face) (font-lock-function-name-face (:foreground "#E06C75") font-lock-function-name-face)) t))"##
        ]],
    )
}

fn atom_one_dark_theme_interactive_call_bypasses_nil_force_gate_and_returns_last_cookie()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_interactive_call_bypasses_nil_force_gate_and_returns_last_cookie",
        r##"(with-temp-buffer
         (setq major-mode 'html-mode)
         (setq-local
          atom-one-dark-theme-force-faces-for-mode
          nil)
         (list
          (commandp
           'atom-one-dark-theme-change-faces-for-mode)
          (interactive-form
           'atom-one-dark-theme-change-faces-for-mode)
          (atom-one-dark-theme-change-faces-for-mode)
          face-remapping-alist
          (call-interactively
           #'atom-one-dark-theme-change-faces-for-mode)
          face-remapping-alist
          (local-variable-p
           'face-remapping-alist)))"##,
        expect![[
            r##"OK (t (interactive nil) nil nil (font-lock-variable-name-face . #1=(:foreground "#D19A66")) ((font-lock-variable-name-face #1# font-lock-variable-name-face) (font-lock-function-name-face (:foreground "#E06C75") font-lock-function-name-face)) t)"##
        ]],
    )
}

fn atom_one_dark_theme_registered_hook_applies_real_html_font_lock_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_registered_hook_applies_real_html_font_lock_workflow",
        r##"(let ((hook-count 0))
         (dolist
             (function after-change-major-mode-hook)
           (when
               (eq
                function
                'atom-one-dark-theme-change-faces-for-mode)
             (setq hook-count
                   (1+ hook-count))))
         (with-temp-buffer
           (insert
            "<section class=\"card\" data-kind=\"primary\">Hello</section>")
           (html-mode)
           (font-lock-ensure)
           (list
            hook-count
            major-mode
            face-remapping-alist
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
             '("section"
               "class"
               "\"card\""
               "data-kind"
               "\"primary\""
               "Hello"))
            (buffer-substring-no-properties
             (point-min)
             (point-max)))))"##,
        expect![[
            r##"OK (1 html-mode ((font-lock-variable-name-face (:foreground "#D19A66") font-lock-variable-name-face) (font-lock-function-name-face (:foreground "#E06C75") font-lock-function-name-face)) (("section" font-lock-function-name-face) ("class" font-lock-variable-name-face) ("\"card\"" font-lock-string-face) ("data-kind" font-lock-variable-name-face) ("\"primary\"" font-lock-string-face) ("Hello" nil)) "<section class=\"card\" data-kind=\"primary\">Hello</section>")"##
        ]],
    )
}

fn atom_one_dark_theme_js2_recipe_remaps_three_faces_with_exact_effective_values() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_one_dark_theme_js2_recipe_remaps_three_faces_with_exact_effective_values",
        r##"(with-temp-buffer
         (setq major-mode 'js2-mode)
         (let ((result
                (atom-one-dark-theme-change-faces-for-mode)))
           (list
            result
            face-remapping-alist
            (mapcar
             (lambda (face)
               (list
                face
                (cdr
                 (assq
                  face
                  face-remapping-alist))))
             '(font-lock-constant-face
               font-lock-doc-face
               font-lock-variable-name-face)))))"##,
        expect![[
            r##"OK ((font-lock-variable-name-face . #1=(:foreground "#ABB2BF")) ((font-lock-variable-name-face . #4=(#1# font-lock-variable-name-face)) (font-lock-doc-face . #3=((:inherit (font-lock-comment-face)) font-lock-doc-face)) (font-lock-constant-face . #2=((:foreground "#D19A66") font-lock-constant-face))) ((font-lock-constant-face #2#) (font-lock-doc-face #3#) (font-lock-variable-name-face #4#)))"##
        ]],
    )
    .fresh_process()
}

fn atom_one_dark_theme_major_mode_changes_clear_old_remaps_and_apply_new_recipe() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_one_dark_theme_major_mode_changes_clear_old_remaps_and_apply_new_recipe",
        r##"(with-temp-buffer
         (setq major-mode 'js2-mode)
         (run-hooks
          'after-change-major-mode-hook)
         (let ((js2
                (copy-tree
                 face-remapping-alist)))
           (kill-all-local-variables)
           (setq major-mode 'html-mode)
           (run-hooks
            'after-change-major-mode-hook)
           (let ((html
                  (copy-tree
                   face-remapping-alist)))
             (kill-all-local-variables)
             (setq major-mode 'text-mode)
             (run-hooks
              'after-change-major-mode-hook)
             (list
              js2
              html
              face-remapping-alist
              (local-variable-p
               'face-remapping-alist)))))"##,
        expect![[
            r##"OK (((font-lock-variable-name-face (:foreground "#ABB2BF") font-lock-variable-name-face) (font-lock-doc-face (:inherit (font-lock-comment-face)) font-lock-doc-face) (font-lock-constant-face (:foreground "#D19A66") font-lock-constant-face)) ((font-lock-variable-name-face (:foreground "#D19A66") font-lock-variable-name-face) (font-lock-function-name-face (:foreground "#E06C75") font-lock-function-name-face)) nil nil)"##
        ]],
    )
    .fresh_process()
}

fn atom_one_dark_theme_repeated_html_remaps_stack_then_cookie_removal_matches_gnu()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_repeated_html_remaps_stack_then_cookie_removal_matches_gnu",
        r##"(with-temp-buffer
         (setq major-mode 'html-mode)
         (let ((first
                (atom-one-dark-theme-change-faces-for-mode))
               second
               after-both
               after-first-removal)
           (setq second
                 (atom-one-dark-theme-change-faces-for-mode))
           (setq after-both
                 (copy-tree
                  face-remapping-alist))
           (face-remap-remove-relative first)
           (setq after-first-removal
                 (copy-tree
                  face-remapping-alist))
           (face-remap-remove-relative second)
           (list
            first
            second
            (equal first second)
            after-both
            after-first-removal
            face-remapping-alist)))"##,
        expect![[
            r##"OK ((font-lock-variable-name-face :foreground "#D19A66") (font-lock-variable-name-face :foreground "#D19A66") t ((font-lock-variable-name-face (:foreground "#D19A66") (:foreground "#D19A66") font-lock-variable-name-face) (font-lock-function-name-face (:foreground "#E06C75") (:foreground "#E06C75") font-lock-function-name-face)) ((font-lock-variable-name-face (:foreground "#D19A66") font-lock-variable-name-face) (font-lock-function-name-face (:foreground "#E06C75") (:foreground "#E06C75") font-lock-function-name-face)) ((font-lock-function-name-face (:foreground "#E06C75") (:foreground "#E06C75") font-lock-function-name-face)))"##
        ]],
    )
}

fn atom_one_dark_theme_remapping_error_propagation_stops_at_exact_failed_call() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_remapping_error_propagation_stops_at_exact_failed_call",
        r##"(mapcar
         (lambda (failure-index)
           (let ((major-mode 'js2-mode)
                 (count 0)
                 calls)
             (cl-letf
                 (((symbol-function
                    'face-remap-add-relative)
                   (lambda
                     (face &rest specs)
                     (setq count
                           (1+ count))
                     (push
                      (cons face specs)
                      calls)
                     (if
                         (= count failure-index)
                         (error
                          "fixture failure %d %S"
                          count face)
                       (list
                        :cookie count face)))))
               (list
                failure-index
                (atom-one-dark-test-error
                 (lambda ()
                   (atom-one-dark-theme-change-faces-for-mode)))
                (nreverse calls)))))
         '(1 2 3 4))"##,
        expect![[
            r##"OK ((1 (:signal error ("fixture failure 1 font-lock-constant-face")) ((font-lock-constant-face :foreground "#D19A66"))) (2 (:signal error ("fixture failure 2 font-lock-doc-face")) ((font-lock-constant-face :foreground "#D19A66") (font-lock-doc-face #1=(:inherit (font-lock-comment-face))))) (3 (:signal error ("fixture failure 3 font-lock-variable-name-face")) ((font-lock-constant-face :foreground "#D19A66") (font-lock-doc-face #1#) (font-lock-variable-name-face :foreground "#ABB2BF"))) (4 (:ok (:cookie 3 font-lock-variable-name-face)) ((font-lock-constant-face :foreground "#D19A66") (font-lock-doc-face #1#) (font-lock-variable-name-face :foreground "#ABB2BF"))))"##
        ]],
    )
    .fresh_process()
}

fn atom_one_dark_theme_unsupported_and_force_disabled_hook_runs_have_no_local_effect()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_unsupported_and_force_disabled_hook_runs_have_no_local_effect",
        r##"(list
         (with-temp-buffer
           (setq major-mode 'special-mode)
           (list
            (atom-one-dark-theme-change-faces-for-mode)
            face-remapping-alist
            (local-variable-p
             'face-remapping-alist)))
         (with-temp-buffer
           (setq major-mode 'js2-mode)
           (setq-local
            atom-one-dark-theme-force-faces-for-mode
            nil)
           (run-hooks
            'after-change-major-mode-hook)
           (list
            face-remapping-alist
            (local-variable-p
             'face-remapping-alist)
            (local-variable-p
             'atom-one-dark-theme-force-faces-for-mode))))"##,
        expect!["OK ((nil nil nil) (nil nil t))"],
    )
}

pub(super) fn remapping_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atom_one_dark_theme_mode_branch_matrix_calls_exact_faces_colors_and_order(),
        atom_one_dark_theme_actual_buffer_local_remapping_alists_match_supported_modes(),
        atom_one_dark_theme_force_gate_accepts_every_truthy_value_but_rejects_nil(),
        atom_one_dark_theme_interactive_call_bypasses_nil_force_gate_and_returns_last_cookie(),
        atom_one_dark_theme_registered_hook_applies_real_html_font_lock_workflow(),
        atom_one_dark_theme_js2_recipe_remaps_three_faces_with_exact_effective_values(),
        atom_one_dark_theme_major_mode_changes_clear_old_remaps_and_apply_new_recipe(),
        atom_one_dark_theme_repeated_html_remaps_stack_then_cookie_removal_matches_gnu(),
        atom_one_dark_theme_remapping_error_propagation_stops_at_exact_failed_call(),
        atom_one_dark_theme_unsupported_and_force_disabled_hook_runs_have_no_local_effect(),
    ]
}
