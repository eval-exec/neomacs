use expect_test::expect;

use super::ParityBatchCase;

fn alan_identifiers_paths_parent_navigation_and_clipboard_follow_real_nested_model()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alan_identifiers_paths_parent_navigation_and_clipboard_follow_real_nested_model",
        r##"(with-temp-buffer
                      (alan-mode)
                      (insert
                       "'root' -> component {\n"
                       "  'accounts' -> collection {\n"
                       "    'balance': number\n"
                       "  }\n"
                       "}\n")
                      (font-lock-ensure)
                      (goto-char (point-min))
                      (search-forward "balance")
                      (backward-char 3)
                      (let ((identifier (thing-at-point 'identifier))
                            (bounds (bounds-of-thing-at-point 'identifier))
                            (path (alan-path))
                            (origin (point)))
                        (alan-copy-path-to-clipboard)
                        (alan-goto-parent)
                        (list
                         identifier
                         (and bounds
                              (buffer-substring-no-properties
                               (car bounds) (cdr bounds)))
                         path
                         (current-kill 0 t)
                         (thing-at-point 'identifier)
                         (line-number-at-pos)
                         (mark)
                         origin)))"##,
        expect![[
            r#"OK ("'balance'" "'balance'" #("'root'.'accounts'" 0 6 (face font-lock-variable-name-face) 7 17 (face font-lock-variable-name-face)) #("'root'.'accounts'" 0 6 (face font-lock-variable-name-face) 7 17 (face font-lock-variable-name-face)) nil 2 61 61)"#
        ]],
    )
}

fn alan_xref_backend_finds_real_definitions_across_open_buffers_and_formats_context()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alan_xref_backend_finds_real_definitions_across_open_buffers_and_formats_context",
        r##"(let ((alan-xref-limit-to-project-scope nil)
                          (first (generate-new-buffer " *alan-first*"))
                          (second (generate-new-buffer " *alan-second*")))
                      (unwind-protect
                          (progn
                            (with-current-buffer first
                              (alan-mode)
                              (insert
                               "'Customer' -> component {\n"
                               "  'name': text\n"
                               "}\n"))
                            (with-current-buffer second
                              (alan-mode)
                              (insert
                               "'Customer' -> component {\n"
                               "  'orders' -> collection\n"
                               "}\n")
                              (font-lock-ensure)
                              (goto-char (point-min))
                              (let ((table
                                     (xref-backend-identifier-completion-table
                                      'alan))
                                    (definitions
                                     (xref-backend-definitions
                                      'alan "'Customer'")))
                                (list
                                 table
                                 (mapcar
                                  (lambda (xref)
                                    (let ((location
                                           (xref-item-location xref)))
                                      (list
                                       (substring-no-properties
                                        (xref-item-summary xref))
                                       (buffer-name
                                        (xref-buffer-location-buffer
                                         location))
                                       (xref-buffer-location-position
                                        location))))
                                  definitions)))))
                        (kill-buffer first)
                        (kill-buffer second)))"##,
        expect![[
            r#"OK (("'orders'" "'Customer'") (("'Customer' component :1" " *alan-second*" 1) ("'Customer' component :1" " *alan-first*" 1)))"#
        ]],
    )
}

fn alan_documentation_include_links_need_thingatpt_which_alan_mode_never_requires()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alan_documentation_include_links_need_thingatpt_which_alan_mode_never_requires",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (target (expand-file-name "included.alan" root))
       visited missing)
  (with-temp-file target
    (insert "'included': text\n"))
  (cl-flet
      ((follow-a-link ()
         (with-temp-buffer
           (insert (format "prefix <<INCLUDE-ALAN[%s]>> suffix" target))
           (goto-char (point-min))
           (search-forward "INCLUDE")
           (condition-case error
               (list :detected (and (alan-documentation-include-link-p) t))
             (error (list :signalled (car error) (cadr error)))))))
    (list
     ;; Nothing has loaded `thingatpt' yet.
     :library-loaded-first (featurep 'thingatpt)
     :autoloaded (list (and (fboundp 'thing-at-point) t)
                       (and (fboundp 'thing-at-point-looking-at) t))
     :bare-session (follow-a-link)
     ;; The same command once the library the package forgot to require is
     ;; present, which is the state every real session is in.
     :after-require
     (progn
       (require 'thingatpt)
       (cl-letf (((symbol-function 'find-file)
                  (lambda (file) (setq visited file) 'visited)))
         (let ((detected (follow-a-link)))
           (with-temp-buffer
             (insert (format "prefix <<INCLUDE-ALAN[%s]>> suffix" target))
             (goto-char (point-min))
             (search-forward "INCLUDE")
             (alan-documentation-follow-include-link-at-point))
           (with-temp-buffer
             (insert "<<INCLUDE-ALAN[missing-file.alan]>>")
             (goto-char 20)
             (setq missing
                   (condition-case error
                       (alan-documentation-follow-include-link-at-point)
                     (user-error (list (car error) (cadr error))))))
           (list detected
                 :visited-the-target (equal visited target)
                 :missing missing)))))))"##,
        expect![[
            r#"OK (:library-loaded-first nil :autoloaded (t nil) :bare-session (:signalled void-function thing-at-point-looking-at) :after-require ((:detected t) :visited-the-target t :missing (user-error "File not found missing-file.alan")))"#
        ]],
    )
    .fresh_process()
}

fn alan_phrase_addition_updates_phrase_and_translation_files_once_and_runs_hook() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alan_phrase_addition_updates_phrase_and_translation_files_once_and_runs_hook",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                           (translations
                            (expand-file-name "translations" root))
                           (phrases
                            (expand-file-name "phrases.alan" root))
                           (english
                            (expand-file-name
                             "translations/en.alan" root))
                           (dutch
                            (expand-file-name
                             "translations/nl.alan" root))
                           (default-directory
                            (file-name-as-directory root))
                           (alan-on-phrase-added-hook nil)
                           hook-events)
                      (make-directory translations t)
                      (with-temp-file phrases
                        (insert "'Existing'\n"))
                      (with-temp-file english
                        (insert "'Existing': \"Existing\"\n"))
                      (with-temp-file dutch
                        (insert "'Existing': \"Bestaand\"\n"))
                      (add-hook
                       'alan-on-phrase-added-hook
                       (lambda () (push 'added hook-events)))
                      (with-temp-buffer
                        (setq buffer-file-name
                              (expand-file-name "views/main.alan" root))
                        (alan-mode)
                        (insert "'New phrase'")
                        (font-lock-ensure)
                        (goto-char 4)
                        (list
                         (alan-add-to-phrases)
                         (alan-add-to-phrases)))
                      (prog1
                          (list
                           (with-temp-buffer
                             (insert-file-contents phrases)
                             (buffer-string))
                           (with-temp-buffer
                             (insert-file-contents english)
                             (buffer-string))
                           (with-temp-buffer
                             (insert-file-contents dutch)
                             (buffer-string))
                           hook-events)
                        (dolist (buffer (buffer-list))
                          (when-let ((file (buffer-file-name buffer)))
                            (when (string-prefix-p root file)
                              (kill-buffer buffer))))))"##,
        expect![[
            r#"OK ("'Existing'\n'New phrase'\n" "'Existing': \"Existing\"\n'New phrase': \"New phrase\"\n" "'Existing': \"Bestaand\"\n'New phrase': \"New phrase\"\n" (added))"#
        ]],
    )
}

fn alan_phrase_removal_updates_the_phrase_and_every_translation_then_runs_hook() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alan_phrase_removal_updates_the_phrase_and_every_translation_then_runs_hook",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                           (translations
                            (expand-file-name "translations" root))
                           (phrases
                            (expand-file-name "phrases.alan" root))
                           (english
                            (expand-file-name
                             "translations/en.alan" root))
                           (dutch
                            (expand-file-name
                             "translations/nl.alan" root))
                           (alan-on-phrase-removed-hook nil)
                           hook-events
                           phrases-buffer)
                      (make-directory translations t)
                      (with-temp-file phrases
                        (insert "'Existing'\n'Remove me'\n'After'\n"))
                      (with-temp-file english
                        (insert
                         "'Existing': \"Existing\"\n"
                         "'Remove me': \"Remove me\"\n"
                         "'After': \"After\"\n"))
                      (with-temp-file dutch
                        (insert
                         "'Existing': \"Bestaand\"\n"
                         "'Remove me': \"Verwijder mij\"\n"
                         "'After': \"Na\"\n"))
                      (add-hook
                       'alan-on-phrase-removed-hook
                       (lambda () (push 'removed hook-events)))
                      (setq phrases-buffer (find-file-noselect phrases))
                      (with-current-buffer phrases-buffer
                        (alan-mode)
                        (font-lock-ensure)
                        (goto-char (point-min))
                        (search-forward "Remove me")
                        (alan-remove-from-phrases))
                      (prog1
                          (list
                           (with-temp-buffer
                             (insert-file-contents phrases)
                             (buffer-string))
                           (with-temp-buffer
                             (insert-file-contents english)
                             (buffer-string))
                           (with-temp-buffer
                             (insert-file-contents dutch)
                             (buffer-string))
                           hook-events)
                        (dolist (buffer (buffer-list))
                          (when-let ((file (buffer-file-name buffer)))
                            (when (string-prefix-p root file)
                              (kill-buffer buffer))))))"##,
        expect![[
            r#"OK ("'Existing'\n'After'\n" "'Existing': \"Existing\"\n'After': \"After\"\n" "'Existing': \"Bestaand\"\n'After': \"Na\"\n" (removed))"#
        ]],
    )
}

pub(super) fn navigation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alan_identifiers_paths_parent_navigation_and_clipboard_follow_real_nested_model(),
        alan_xref_backend_finds_real_definitions_across_open_buffers_and_formats_context(),
        alan_documentation_include_links_need_thingatpt_which_alan_mode_never_requires(),
        alan_phrase_addition_updates_phrase_and_translation_files_once_and_runs_hook(),
        alan_phrase_removal_updates_the_phrase_and_every_translation_then_runs_hook(),
    ]
}
