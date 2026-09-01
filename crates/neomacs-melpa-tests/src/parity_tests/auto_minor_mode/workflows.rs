use expect_test::expect;

use super::ParityBatchCase;

fn auto_minor_mode_real_theme_file_combines_major_filename_and_magic_selection() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_minor_mode_real_theme_file_combines_major_filename_and_magic_selection",
        r##"(let*
                             ((root
                               (auto-minor-mode-test-root
                                "theme-file"))
                              (file
                               (expand-file-name
                                "midnight-theme.el"
                                root))
                              buffer)
                           (auto-minor-mode-test-write
                            file
                            ";;; midnight-theme.el --- Theme\n(deftheme midnight)\n")
                           (auto-minor-mode-test-reset)
                           (let
                               ((auto-mode-alist
                                 '(("\\.el\\'"
                                    . emacs-lisp-mode)))
                                (auto-minor-mode-alist
                                 '(("-theme\\.el\\'"
                                    . auto-minor-mode-test-alpha-mode)))
                                (auto-minor-mode-magic-alist
                                 '(("\\`;;; .*Theme"
                                    . auto-minor-mode-test-beta-mode))))
                             (unwind-protect
                                 (progn
                                   (setq
                                    buffer
                                    (find-file-noselect file))
                                   (with-current-buffer buffer
                                     (list
                                      (file-name-nondirectory
                                       buffer-file-name)
                                      major-mode
                                      auto-minor-mode-test-alpha-mode
                                      auto-minor-mode-test-beta-mode
                                      (nreverse
                                       auto-minor-mode-test-events)
                                      (buffer-substring-no-properties
                                       (point-min)
                                       (point-max))
                                      (point)
                                      (buffer-modified-p))))
                               (when (buffer-live-p buffer)
                                 (with-current-buffer buffer
                                   (set-buffer-modified-p nil))
                                 (kill-buffer buffer))
                               (delete-directory root t))))"##,
        expect![[
            r#"OK ("midnight-theme.el" emacs-lisp-mode t t ((:alpha 1 t 1 emacs-lisp-mode) (:beta 1 t 1 emacs-lisp-mode)) ";;; midnight-theme.el --- Theme\n(deftheme midnight)\n" 1 nil)"#
        ]],
    )
}

fn auto_minor_mode_two_real_project_files_keep_minor_mode_state_buffer_local() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_two_real_project_files_keep_minor_mode_state_buffer_local",
        r##"(let*
                             ((root
                               (auto-minor-mode-test-root
                                "project-buffers"))
                              (secure
                               (expand-file-name
                                "production.secure"
                                root))
                              (ordinary
                               (expand-file-name
                                "development.txt"
                                root))
                              secure-buffer
                              ordinary-buffer)
                           (auto-minor-mode-test-write
                            secure
                            "SECRET service-token\n")
                           (auto-minor-mode-test-write
                            ordinary
                            "public notes\n")
                           (auto-minor-mode-test-reset)
                           (let
                               ((auto-mode-alist
                                 '(("\\.secure\\'"
                                    . conf-mode)
                                   ("\\.txt\\'"
                                    . text-mode)))
                                (auto-minor-mode-alist
                                 '(("\\.secure\\'"
                                    . auto-minor-mode-test-alpha-mode)))
                                (auto-minor-mode-magic-alist
                                 '(("\\`SECRET"
                                    . auto-minor-mode-test-beta-mode))))
                             (unwind-protect
                                 (progn
                                   (setq
                                    secure-buffer
                                    (find-file-noselect secure)
                                    ordinary-buffer
                                    (find-file-noselect ordinary))
                                   (list
                                    (with-current-buffer secure-buffer
                                      (list
                                       major-mode
                                       auto-minor-mode-test-alpha-mode
                                       auto-minor-mode-test-beta-mode
                                       (local-variable-p
                                        'auto-minor-mode-test-alpha-mode)
                                       (local-variable-p
                                        'auto-minor-mode-test-beta-mode)))
                                    (with-current-buffer ordinary-buffer
                                      (list
                                       major-mode
                                       auto-minor-mode-test-alpha-mode
                                       auto-minor-mode-test-beta-mode
                                       (local-variable-p
                                        'auto-minor-mode-test-alpha-mode)
                                       (local-variable-p
                                        'auto-minor-mode-test-beta-mode)))
                                    auto-minor-mode-test-alpha-mode
                                    auto-minor-mode-test-beta-mode
                                    (nreverse
                                     auto-minor-mode-test-events)))
                               (dolist
                                   (buffer
                                    (list
                                     secure-buffer
                                     ordinary-buffer))
                                 (when (buffer-live-p buffer)
                                   (with-current-buffer buffer
                                     (set-buffer-modified-p nil))
                                   (kill-buffer buffer)))
                               (delete-directory root t))))"##,
        expect![
            "OK ((conf-space-mode t t t t) (text-mode nil nil nil nil) nil nil ((:alpha 1 t 1 conf-space-mode) (:beta 1 t 1 conf-space-mode)))"
        ],
    )
}

fn auto_minor_mode_real_numbered_and_simple_backup_files_match_the_base_suffix() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_minor_mode_real_numbered_and_simple_backup_files_match_the_base_suffix",
        r##"(let*
                             ((root
                               (auto-minor-mode-test-root
                                "backup-files"))
                              (numbered
                               (expand-file-name
                                "deploy.ammtest.~17~"
                                root))
                              (simple
                               (expand-file-name
                                "deploy.ammtest~"
                                root))
                              buffers)
                           (auto-minor-mode-test-write
                            numbered
                            "numbered backup\n")
                           (auto-minor-mode-test-write
                            simple
                            "simple backup\n")
                           (auto-minor-mode-test-reset)
                           (let
                               ((auto-mode-alist nil)
                                (auto-minor-mode-alist
                                 '(("\\.ammtest\\'"
                                    . auto-minor-mode-test-alpha-mode)))
                                (auto-minor-mode-magic-alist nil))
                             (unwind-protect
                                 (progn
                                   (setq
                                    buffers
                                    (mapcar
                                     #'find-file-noselect
                                     (list numbered simple)))
                                   (list
                                    (mapcar
                                     (lambda (buffer)
                                       (with-current-buffer buffer
                                         (list
                                          (file-name-nondirectory
                                           buffer-file-name)
                                          (file-name-nondirectory
                                           (auto-minor-mode--plain-filename
                                            buffer-file-name))
                                          auto-minor-mode-test-alpha-mode
                                          (local-variable-p
                                           'auto-minor-mode-test-alpha-mode))))
                                     buffers)
                                    (nreverse
                                     auto-minor-mode-test-events)))
                               (dolist (buffer buffers)
                                 (when (buffer-live-p buffer)
                                   (with-current-buffer buffer
                                     (set-buffer-modified-p nil))
                                   (kill-buffer buffer)))
                               (delete-directory root t))))"##,
        expect![[
            r#"OK ((("deploy.ammtest.~17~" "deploy.ammtest" t t) ("deploy.ammtest~" "deploy.ammtest" t t)) ((:alpha 1 t 1 fundamental-mode) (:alpha 1 t 1 fundamental-mode)))"#
        ]],
    )
}

fn auto_minor_mode_existing_restriction_treats_its_visible_header_as_magic_start() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_minor_mode_existing_restriction_treats_its_visible_header_as_magic_start",
        r##"(with-temp-buffer
                           (auto-minor-mode-test-reset)
                           (insert
                            "ignored preamble\n"
                            "#!service\n"
                            "enabled=true\n"
                            "ignored trailer\n")
                           (goto-char (point-min))
                           (forward-line 1)
                           (let
                               ((visible-start (point)))
                             (forward-line 2)
                             (narrow-to-region
                              visible-start
                              (point))
                             (goto-char (+ (point-min) 4))
                             (let
                                 ((original-point (point))
                                  (original-min (point-min))
                                  (original-max (point-max))
                                  (auto-minor-mode-magic-alist
                                   '(("\\`#!service"
                                      . auto-minor-mode-test-alpha-mode))))
                               (auto-minor-mode-set)
                               (list
                                auto-minor-mode-test-alpha-mode
                                (nreverse
                                 auto-minor-mode-test-events)
                                (= original-point (point))
                                (= original-min (point-min))
                                (= original-max (point-max))
                                (buffer-string)))))"##,
        expect![[
            r##"OK (t ((:alpha 1 t 18 fundamental-mode)) t t t "#!service\nenabled=true\n")"##
        ]],
    )
}

fn auto_minor_mode_real_revisit_without_selected_major_reactivates_even_when_keep_requested()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_minor_mode_real_revisit_without_selected_major_reactivates_even_when_keep_requested",
        r##"(let*
                             ((root
                               (auto-minor-mode-test-root
                                "revisit"))
                              (file
                               (expand-file-name
                                "service.ammtest"
                                root))
                              buffer)
                           (auto-minor-mode-test-write
                            file
                            "service=true\n")
                           (auto-minor-mode-test-reset)
                           (let
                               ((auto-mode-alist nil)
                                (auto-minor-mode-alist
                                 '(("\\.ammtest\\'"
                                    . auto-minor-mode-test-alpha-mode)))
                                (auto-minor-mode-magic-alist nil))
                             (unwind-protect
                                 (progn
                                   (setq
                                    buffer
                                    (find-file-noselect file))
                                   (with-current-buffer buffer
                                     (let
                                         ((after-open
                                           (length
                                            auto-minor-mode-test-events)))
                                       (set-auto-mode t)
                                       (let
                                           ((after-keep
                                             (length
                                              auto-minor-mode-test-events)))
                                         (set-auto-mode)
                                         (list
                                          after-open
                                          after-keep
                                          (length
                                           auto-minor-mode-test-events)
                                          auto-minor-mode-test-alpha-mode
                                          (nreverse
                                           auto-minor-mode-test-events))))))
                               (when (buffer-live-p buffer)
                                 (with-current-buffer buffer
                                   (set-buffer-modified-p nil))
                                 (kill-buffer buffer))
                               (delete-directory root t))))"##,
        expect![
            "OK (1 2 3 t ((:alpha 1 t 1 fundamental-mode) (:alpha 1 t 1 fundamental-mode) (:alpha 1 t 1 fundamental-mode)))"
        ],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_minor_mode_real_theme_file_combines_major_filename_and_magic_selection(),
        auto_minor_mode_two_real_project_files_keep_minor_mode_state_buffer_local(),
        auto_minor_mode_real_numbered_and_simple_backup_files_match_the_base_suffix(),
        auto_minor_mode_existing_restriction_treats_its_visible_header_as_magic_start(),
        auto_minor_mode_real_revisit_without_selected_major_reactivates_even_when_keep_requested(),
    ]
}
