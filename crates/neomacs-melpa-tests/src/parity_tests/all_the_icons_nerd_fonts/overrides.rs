use expect_test::expect;

use super::ParityBatchCase;

fn readme_preference_routes_specific_overrides_family_conversions_and_real_fallbacks()
-> ParityBatchCase {
    ParityBatchCase::value(
        "readme_preference_routes_specific_overrides_family_conversions_and_real_fallbacks",
        r##"(let* ((override-map
                      (all-the-icons-nerd-fonts--build-override-map))
                     (nerd-material
                      (all-the-icons-nerd-fonts--get-nerd-data-alist
                       'all-the-icons-nerd-md))
                     (fallback
                      (car
                       (seq-find
                        (lambda (entry)
                          (let ((name (car entry)))
                            (and
                             (not
                              (gethash
                               (concat
                                "all-the-icons-material-"
                                name)
                               override-map))
                             (not
                              (assoc
                               (string-replace "_" "-" name)
                               nerd-material)))))
                        all-the-icons-data/material-icons-alist)))
                     (describe
                      (lambda (icon)
                        (list
                         (substring-no-properties icon)
                         (string-to-list
                          (substring-no-properties icon))
                         (all-the-icons-icon-family icon)
                         (get-text-property 0 'face icon)
                         (get-text-property 0 'display icon))))
                     (before
                      (list
                       (funcall
                        describe
                        (all-the-icons-faicon
                         "github"
                         :face 'font-lock-constant-face))
                       (funcall
                        describe
                        (all-the-icons-material
                         "format_align_left"
                         :face 'font-lock-keyword-face))
                       (funcall
                        describe
                        (all-the-icons-material
                         fallback
                         :face 'warning))))
                     preferred)
               (unwind-protect
                   (progn
                     (all-the-icons-nerd-fonts-prefer '())
                     (setq
                      preferred
                      (list
                       (funcall
                        describe
                        (all-the-icons-faicon
                         "github"
                         :face 'font-lock-constant-face))
                       (funcall
                        describe
                        (all-the-icons-material
                         "format_align_left"
                         :face 'font-lock-keyword-face))
                       (funcall
                        describe
                        (all-the-icons-material
                         fallback
                         :face 'warning))))
                     (list fallback before preferred))
                 (all-the-icons-nerd-fonts-unprefer)))"##,
        expect![[
            r#"OK ("3d_rotation" (("" (61595) "FontAwesome" (:family "FontAwesome" :height 1.2 :inherit font-lock-constant-face) (raise -0.24)) ("" (57910) "Material Icons" (:family "Material Icons" :height 1.2 :inherit font-lock-keyword-face) (raise -0.24)) ("" (59469) "Material Icons" (:family "Material Icons" :height 1.2 :inherit warning) (raise -0.24))) (("" (60036) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit font-lock-constant-face) (raise -0.24)) ("󰉢" (983650) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit font-lock-keyword-face) (raise -0.24)) ("" (59469) "Material Icons" (:family "Material Icons" :height 1.2 :inherit warning) (raise -0.24))))"#
        ]],
    )
    .fresh_process()
}

fn user_override_customization_redirects_a_real_direct_call_with_arguments_intact()
-> ParityBatchCase {
    ParityBatchCase::value(
        "user_override_customization_redirects_a_real_direct_call_with_arguments_intact",
        r##"(let
               ((all-the-icons-nerd-fonts-overrides
                 '((all-the-icons-faicon
                    "github"
                    all-the-icons-nerd-dev
                    "rust")))
                (all-the-icons-nerd-fonts-convert-families
                 '((all-the-icons-faicon
                    . all-the-icons-nerd-fa)))
                icon)
               (unwind-protect
                   (progn
                     (all-the-icons-nerd-fonts-prefer '())
                     (setq
                      icon
                      (all-the-icons-faicon
                       "github"
                       :face 'font-lock-type-face
                       :height 1.5
                       :v-adjust 0.1))
                     (list
                      (substring-no-properties icon)
                      (string-to-list
                       (substring-no-properties icon))
                      (all-the-icons-icon-family icon)
                      (get-text-property 0 'face icon)
                      (get-text-property 0 'display icon)
                      all-the-icons-nerd-fonts--advice-enabled
                      (not
                       (null
                        (advice-member-p
                         'all-the-icons-nerd-fonts
                         'all-the-icons-faicon)))))
                 (all-the-icons-nerd-fonts-unprefer)))"##,
        expect![[
            r#"OK ("" (59304) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.7999999999999998 :inherit font-lock-type-face) (raise 0.12) t t)"#
        ]],
    )
}

fn unprefer_restores_original_direct_rendering_after_a_real_preference_session() -> ParityBatchCase
{
    ParityBatchCase::value(
        "unprefer_restores_original_direct_rendering_after_a_real_preference_session",
        r##"(let* ((describe
                      (lambda (icon)
                        (list
                         (substring-no-properties icon)
                         (string-to-list
                          (substring-no-properties icon))
                         (all-the-icons-icon-family icon)
                         (get-text-property 0 'face icon))))
                     (before
                      (funcall
                       describe
                       (all-the-icons-faicon
                        "github"
                        :face 'success)))
                     preferred
                     restored)
               (unwind-protect
                   (progn
                     (all-the-icons-nerd-fonts-prefer '())
                     (setq
                      preferred
                      (funcall
                       describe
                       (all-the-icons-faicon
                        "github"
                        :face 'success)))
                     (all-the-icons-nerd-fonts-unprefer)
                     (setq
                      restored
                      (funcall
                       describe
                       (all-the-icons-faicon
                        "github"
                        :face 'success)))
                     (list
                      before
                      preferred
                      restored
                      (equal before restored)
                      all-the-icons-nerd-fonts--advice-enabled
                      (advice-member-p
                       'all-the-icons-nerd-fonts
                       'all-the-icons-faicon)))
                 (all-the-icons-nerd-fonts-unprefer)))"##,
        expect![[
            r#"OK (("" (61595) "FontAwesome" (:family "FontAwesome" :height 1.2 :inherit success)) ("" (60036) "Symbols Nerd Font" (:family "Symbols Nerd Font" :height 1.2 :inherit success)) ("" (61595) "FontAwesome" (:family "FontAwesome" :height 1.2 :inherit success)) t nil nil)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn overrides_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        readme_preference_routes_specific_overrides_family_conversions_and_real_fallbacks(),
        user_override_customization_redirects_a_real_direct_call_with_arguments_intact(),
        unprefer_restores_original_direct_rendering_after_a_real_preference_session(),
    ]
}
