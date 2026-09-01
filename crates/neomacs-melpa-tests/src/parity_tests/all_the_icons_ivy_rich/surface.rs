use expect_test::expect;

use super::ParityBatchCase;

fn pinned_package_loads_its_real_ivy_rich_ivy_and_all_the_icons_dependency_graph() -> ParityBatchCase
{
    ParityBatchCase::value(
        "pinned_package_loads_its_real_ivy_rich_ivy_and_all_the_icons_dependency_graph",
        r##"(let ((packages
                    '(all-the-icons-ivy-rich
                      ivy-rich
                      ivy
                      all-the-icons)))
               (list
                (mapcar
                 (lambda (package)
                   (list
                    package
                    (featurep package)
                    (let ((description
                           (cadr (assq package package-alist))))
                      (and description
                           (package-version-join
                            (package-desc-version description))))
                    (file-name-nondirectory
                     (or (locate-library (symbol-name package)) ""))))
                 packages)
                (mapcar
                 (lambda (feature)
                   (and (featurep feature) feature))
                 '(cl-lib subr-x package bookmark project))))"##,
        expect![[
            r#"OK (((all-the-icons-ivy-rich t "20230420.1234" "all-the-icons-ivy-rich.el") (ivy-rich t "20230425.1422" "ivy-rich.el") (ivy t "20260413.2102" "ivy.el") (all-the-icons t "20250527.927" "all-the-icons.el")) (cl-lib subr-x package bookmark project))"#
        ]],
    )
}

fn readme_color_size_and_icon_customizations_change_a_rendered_file_candidate() -> ParityBatchCase {
    ParityBatchCase::value(
        "readme_color_size_and_icon_customizations_change_a_rendered_file_candidate",
        r##"(progn
               (require 'cl-lib)
               (cl-letf
                   (((symbol-function 'display-graphic-p)
                     (lambda (&optional _frame) t)))
                 (let* ((all-the-icons-ivy-rich-icon t)
                        (all-the-icons-ivy-rich-color-icon t)
                        (all-the-icons-ivy-rich-icon-size 1.4)
                        (colored
                         (all-the-icons-ivy-rich-file-icon "README.md"))
                        (all-the-icons-ivy-rich-color-icon nil)
                        (all-the-icons-ivy-rich-icon-size 0.75)
                        (plain
                         (all-the-icons-ivy-rich-file-icon "README.md"))
                        (all-the-icons-ivy-rich-icon nil)
                        (disabled
                         (all-the-icons-ivy-rich-file-icon "README.md")))
                   (list
                    (list
                     (substring-no-properties colored)
                     (get-text-property 1 'face colored)
                     (get-text-property 1 'display colored))
                    (list
                     (substring-no-properties plain)
                     (get-text-property 1 'face plain)
                     (get-text-property 1 'display plain))
                    disabled))))"##,
        expect![[
            r#"OK ((" " (:inherit all-the-icons-lcyan :family "github-octicons" :height 1.4) #1=(raise 0.0)) (" " (:inherit all-the-icons-ivy-rich-icon-face :family "github-octicons" :height 0.75) #1#) nil)"#
        ]],
    )
}

fn enabled_package_mode_builds_and_applies_real_file_and_buffer_transformers() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabled_package_mode_builds_and_applies_real_file_and_buffer_transformers",
        r##"(let* ((root
                     (file-name-as-directory
                      (expand-file-name
                       "air"
                       (getenv "TMPDIR"))))
                    (source-directory
                     (file-name-as-directory
                      (expand-file-name "src" root)))
                    (file
                     (expand-file-name
                      "work-notes.el"
                      source-directory))
                    (buffer-name "work-notes.el")
                    (ivy--directory source-directory)
                    (ivy-last
                     (make-ivy-state :caller 'counsel-find-file))
                    (all-the-icons-ivy-rich-project nil)
                    buffer
                    rendered)
               (unwind-protect
                   (progn
                     (when (file-exists-p root)
                       (delete-directory root t))
                     (make-directory
                      (expand-file-name ".git" root)
                      t)
                     (make-directory source-directory t)
                     (with-temp-file file
                       (insert "(message \"ready\")\n"))
                     (set-file-modes file #o640)
                     (set-file-times
                      file
                      (encode-time 0 34 12 2 1 2024 t))
                     (setq buffer (find-file-noselect file))
                     (with-current-buffer buffer
                       (emacs-lisp-mode)
                       (goto-char (point-max))
                       (insert ";; unsaved\n"))
                     (cl-letf
                         (((symbol-function 'display-graphic-p)
                           (lambda (&optional _frame) t)))
                       (all-the-icons-ivy-rich-mode 1)
                       (let* ((file-configuration
                               (cadr
                                (memq
                                 'counsel-find-file
                                 ivy-rich-display-transformers-list)))
                              (buffer-configuration
                               (cadr
                                (memq
                                 'ivy-switch-buffer
                                 ivy-rich-display-transformers-list)))
                              (file-transformer
                               (ivy-rich-build-transformer
                                'counsel-find-file
                                file-configuration))
                              (buffer-transformer
                               (ivy-rich-build-transformer
                                'ivy-switch-buffer
                                buffer-configuration))
                              (file-fields
                               (split-string
                                (funcall file-transformer
                                         "work-notes.el")
                                "\t"))
                              (buffer-fields
                               (split-string
                                (funcall buffer-transformer
                                         buffer-name)
                                "\t")))
                         (setq
                          rendered
                          (list
                           (functionp file-transformer)
                           (functionp buffer-transformer)
                           (mapcar
                            (lambda (field)
                              (string-trim-right
                               (substring-no-properties field)))
                            file-fields)
                           (list
                            (substring-no-properties
                             (nth 0 buffer-fields))
                            (string-trim-right
                             (substring-no-properties
                              (nth 1 buffer-fields)))
                            (string-trim
                             (substring-no-properties
                              (nth 2 buffer-fields)))
                            (string-trim
                             (substring-no-properties
                              (nth 3 buffer-fields)))
                            (string-trim-right
                             (substring-no-properties
                              (nth 4 buffer-fields)))
                            (string-trim
                             (substring-no-properties
                              (nth 5 buffer-fields)))
                            (file-name-nondirectory
                             (directory-file-name
                              (string-trim
                               (substring-no-properties
                                (nth 6 buffer-fields)))))
                            (get-text-property
                             1 'face (nth 0 buffer-fields))
                            (get-text-property
                             0 'face (nth 4 buffer-fields))))))))
                 (when all-the-icons-ivy-rich-mode
                   (all-the-icons-ivy-rich-mode -1))
                 (when (buffer-live-p buffer)
                   (with-current-buffer buffer
                     (set-buffer-modified-p nil))
                   (kill-buffer buffer))
                 (when (file-exists-p root)
                   (delete-directory root t)))
               rendered)"##,
        expect![[
            r#"OK (t t (" " "work-notes.el" "" "-rw-r-----" "18" "Jan 02 12:34") (" " "work-notes.el" "29" "*" "" "air" "src" (:inherit all-the-icons-purple :family "file-icons" :height 1.0) all-the-icons-ivy-rich-major-mode-face))"#
        ]],
    )
}

fn global_mode_enable_reload_and_disable_manage_hooks_advice_and_transformers() -> ParityBatchCase {
    ParityBatchCase::value(
        "global_mode_enable_reload_and_disable_manage_hooks_advice_and_transformers",
        r##"(let ((original
                    ivy-rich-display-transformers-list)
                   enabled
                   reloaded
                   disabled)
               (unwind-protect
                   (progn
                     (all-the-icons-ivy-rich-mode 1)
                     (setq
                      enabled
                      (list
                       all-the-icons-ivy-rich-mode
                       (not
                        (null
                         (memq
                          #'all-the-icons-ivy-rich-minibuffer-align-icons
                          minibuffer-setup-hook)))
                       (not
                        (null
                         (advice-member-p
                          #'all-the-icons-ivy-rich-kill-buffer
                          #'kill-buffer)))
                       (eq
                        ivy-rich-display-transformers-list
                        all-the-icons-ivy-rich-display-transformers-list)))
                     (all-the-icons-ivy-rich-reload)
                     (setq
                      reloaded
                      (list
                       all-the-icons-ivy-rich-mode
                       (not
                        (null
                         (advice-member-p
                          #'all-the-icons-ivy-rich-kill-buffer
                          #'kill-buffer)))
                       (eq
                        ivy-rich-display-transformers-list
                        all-the-icons-ivy-rich-display-transformers-list)))
                     (all-the-icons-ivy-rich-mode -1)
                     (setq
                      disabled
                      (list
                       all-the-icons-ivy-rich-mode
                       (memq
                        #'all-the-icons-ivy-rich-minibuffer-align-icons
                        minibuffer-setup-hook)
                       (advice-member-p
                        #'all-the-icons-ivy-rich-kill-buffer
                        #'kill-buffer)
                       (eq
                        ivy-rich-display-transformers-list
                        original)))
                     (list enabled reloaded disabled))
                 (when all-the-icons-ivy-rich-mode
                   (all-the-icons-ivy-rich-mode -1))))"##,
        expect!["OK ((t t t t) (t t t) (nil nil nil t))"],
    )
}

fn graphical_icon_gate_and_buffer_alignment_follow_runtime_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "graphical_icon_gate_and_buffer_alignment_follow_runtime_state",
        r##"(progn
               (require 'cl-lib)
               (let ((all-the-icons-ivy-rich-icon t)
                     graphical
                     nongraphical
                     disabled
                     aligned)
                 (cl-letf
                     (((symbol-function 'display-graphic-p)
                       (lambda (&optional _frame) t)))
                   (setq graphical
                         (all-the-icons-ivy-rich-icon-displayable)))
                 (cl-letf
                     (((symbol-function 'display-graphic-p)
                       (lambda (&optional _frame) nil)))
                   (setq nongraphical
                         (all-the-icons-ivy-rich-icon-displayable)))
                 (setq all-the-icons-ivy-rich-icon nil)
                 (cl-letf
                     (((symbol-function 'display-graphic-p)
                       (lambda (&optional _frame) t)))
                   (setq disabled
                         (all-the-icons-ivy-rich-icon-displayable)))
                 (with-temp-buffer
                   (setq tab-width 8)
                   (all-the-icons-ivy-rich-minibuffer-align-icons)
                   (setq aligned tab-width))
                 (list graphical nongraphical disabled aligned)))"##,
        expect!["OK (t nil nil 1)"],
    )
}

pub(super) fn surface_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        pinned_package_loads_its_real_ivy_rich_ivy_and_all_the_icons_dependency_graph(),
        readme_color_size_and_icon_customizations_change_a_rendered_file_candidate(),
        enabled_package_mode_builds_and_applies_real_file_and_buffer_transformers(),
        global_mode_enable_reload_and_disable_manage_hooks_advice_and_transformers(),
        graphical_icon_gate_and_buffer_alignment_follow_runtime_state(),
    ]
}
