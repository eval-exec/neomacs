use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_chunk_real_menu_navigates_and_completes_a_python_attribute() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_real_menu_navigates_and_completes_a_python_attribute",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (python-mode)
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-expand-on-auto-complete nil)
                                  (ac-sources
                                   '(ac-source-chunk-list))
                                  (ac-chunk-list
                                   '("os.path.abspath"
                                     "os.path.altsep"
                                     "os.path.basename"
                                     "sys.path.append")))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "resolved = os.path.a")
                                    (auto-complete)
                                    (let ((initial
                                           (list
                                            (buffer-substring-no-properties
                                             (line-beginning-position)
                                             (line-end-position))
                                            ac-prefix
                                            (mapcar
                                             (lambda (candidate)
                                               (list
                                                (substring-no-properties
                                                 candidate)
                                                (popup-item-symbol
                                                 candidate)))
                                             ac-candidates)
                                            (popup-live-p ac-menu)
                                            (substring-no-properties
                                             (ac-selected-candidate)))))
                                      (ac-next)
                                      (let ((selected
                                             (substring-no-properties
                                              (ac-selected-candidate)))
                                            (completed
                                             (ac-complete)))
                                        (list
                                         initial
                                         selected
                                         (substring-no-properties
                                          completed)
                                         (buffer-substring-no-properties
                                          (line-beginning-position)
                                          (line-end-position))
                                         ac-menu
                                         ac-completing
                                         ac-prefix))))
                                (auto-complete-mode -1)))))"##,
        expect![[
            r#"OK (("resolved = os.path.a" "os.path.a" (("os.path.abspath" "c") ("os.path.altsep" "c")) t "os.path.abspath") "os.path.altsep" "os.path.altsep" "resolved = os.path.altsep" nil nil nil)"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_chunk_real_menu_incrementally_recomputes_after_attribute_typing() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_chunk_real_menu_incrementally_recomputes_after_attribute_typing",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (python-mode)
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-expand-on-auto-complete nil)
                                  (ac-sources
                                   '(ac-source-chunk-list))
                                  (ac-chunk-list
                                   '("request.headers.accept"
                                     "request.headers.authorization"
                                     "request.host"
                                     "response.headers.accept")))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "request.h")
                                    (auto-complete)
                                    (let ((initial
                                           (list
                                            ac-prefix
                                            (mapcar
                                             #'substring-no-properties
                                             ac-candidates))))
                                      (insert "e")
                                      (setq ac-prefix
                                            (buffer-substring-no-properties
                                             ac-point
                                             (point)))
                                      (ac-update t)
                                      (let ((headers
                                             (list
                                              ac-prefix
                                              (mapcar
                                               #'substring-no-properties
                                               ac-candidates))))
                                        (insert "aders.a")
                                        (setq ac-prefix
                                              (buffer-substring-no-properties
                                               ac-point
                                               (point)))
                                        (ac-update t)
                                        (let ((attribute
                                               (list
                                                ac-prefix
                                                (mapcar
                                                 #'substring-no-properties
                                                 ac-candidates))))
                                          (ac-complete)
                                          (list
                                           initial
                                           headers
                                           attribute
                                           (buffer-substring-no-properties
                                            (line-beginning-position)
                                            (line-end-position))
                                           ac-menu
                                           ac-completing)))))
                                (auto-complete-mode -1)))))"##,
        expect![[
            r#"OK (("request.h" ("request.headers.accept" "request.headers.authorization" "request.host")) ("request.he" ("request.headers.accept" "request.headers.authorization")) ("request.headers.a" ("request.headers.accept" "request.headers.authorization")) "request.headers.accept" nil nil)"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_chunk_real_command_completes_from_its_declared_source_only() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_real_command_completes_from_its_declared_source_only",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (python-mode)
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-expand-on-auto-complete nil)
                                  (ac-sources
                                   '(ac-source-filename))
                                  (ac-chunk-list
                                   '("database.connection.close"
                                     "database.connection.commit"
                                     "database.cursor.close")))
                              (unwind-protect
                                  (progn
                                    (auto-complete-mode 1)
                                    (insert "database.connection.c")
                                    (let ((started
                                           (ac-complete-chunk-list)))
                                      (list
                                       started
                                       ac-prefix
                                       (mapcar
                                        #'substring-no-properties
                                        ac-candidates)
                                       (substring-no-properties
                                        (ac-selected-candidate))
                                       (mapcar
                                        (lambda (source)
                                          (if
                                              (symbolp source)
                                              source
                                            :anonymous))
                                        ac-sources))))
                                (auto-complete-mode -1)))))"##,
        expect![[
            r#"OK (t "database.connection.c" ("database.connection.close" "database.connection.commit") "database.connection.close" (ac-source-filename))"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_chunk_dictionary_swap_drives_a_real_completion_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_dictionary_swap_drives_a_real_completion_session",
        r##"(save-window-excursion
                          (with-temp-buffer
                            (switch-to-buffer
                             (current-buffer))
                            (fundamental-mode)
                            (let ((ac-use-comphist nil)
                                  (ac-use-quick-help nil)
                                  (ac-auto-show-menu t)
                                  (ac-expand-on-auto-complete nil)
                                  (ac-sources
                                   '(ac-source-dictionary
                                     ac-source-filename))
                                  (dictionary
                                   '("service.cache.clear"
                                     "service.cache.close"
                                     "service.config.clear")))
                              (unwind-protect
                                  (cl-letf
                                      (((symbol-function
                                         'ac-buffer-dictionary)
                                        (lambda ()
                                          dictionary)))
                                    (auto-complete-mode 1)
                                    (ac-use-dictionary-chunk)
                                    (insert "service.cache.c")
                                    (auto-complete)
                                    (let ((session
                                           (list
                                            ac-sources
                                            ac-prefix
                                            (mapcar
                                             (lambda (candidate)
                                               (list
                                                (substring-no-properties
                                                 candidate)
                                                (popup-item-symbol
                                                 candidate)))
                                             ac-candidates))))
                                      (ac-next)
                                      (ac-complete)
                                      (list
                                       session
                                       (buffer-substring-no-properties
                                        (line-beginning-position)
                                        (line-end-position))
                                       ac-menu
                                       ac-completing)))
                                (auto-complete-mode -1)))))"##,
        expect![[
            r#"OK (((ac-source-dictionary-chunk ac-source-filename) "service.cache.c" (("service.cache.clear" "c") ("service.cache.close" "c"))) "service.cache.close" nil nil)"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_chunk_two_live_buffers_keep_independent_dictionaries_and_completion_results()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_two_live_buffers_keep_independent_dictionaries_and_completion_results",
        r##"(save-window-excursion
                          (let ((first
                                 (generate-new-buffer " *chunk-session-first*"))
                                (second
                                 (generate-new-buffer " *chunk-session-second*")))
                            (unwind-protect
                                (mapcar
                                 (lambda (fixture)
                                   (let ((buffer
                                          (nth 0 fixture))
                                         (text
                                          (nth 1 fixture))
                                         (dictionary
                                          (nth 2 fixture)))
                                     (with-current-buffer buffer
                                       (switch-to-buffer buffer)
                                       (python-mode)
                                       (let ((ac-use-comphist nil)
                                             (ac-use-quick-help nil)
                                             (ac-auto-show-menu t)
                                             (ac-expand-on-auto-complete nil)
                                             (ac-sources
                                              '(ac-source-chunk-list)))
                                         (setq ac-chunk-list dictionary)
                                         (auto-complete-mode 1)
                                         (insert text)
                                         (auto-complete)
                                         (let ((result
                                                (list
                                                 (buffer-name)
                                                 ac-prefix
                                                 (mapcar
                                                  #'substring-no-properties
                                                  ac-candidates)
                                                 (ac-chunk-list))))
                                           (ac-complete)
                                           (auto-complete-mode -1)
                                           (append
                                            result
                                            (list
                                             (buffer-string))))))))
                                 (list
                                  (list
                                   first
                                   "api.users.f"
                                   '("api.users.fetch"
                                     "api.users.find"
                                     "api.groups.fetch"))
                                  (list
                                   second
                                   "api.orders.f"
                                   '("api.orders.fetch"
                                     "api.orders.fulfill"
                                     "api.users.fetch"))))
                              (kill-buffer first)
                              (kill-buffer second))))"##,
        expect![[
            r#"OK ((" *chunk-session-first*" "api.users.f" ("api.users.fetch" "api.users.find") ("api.users.fetch" "api.users.find" "api.groups.fetch") "api.users.fetch") (" *chunk-session-second*" "api.orders.f" ("api.orders.fetch" "api.orders.fulfill") ("api.orders.fetch" "api.orders.fulfill" "api.users.fetch") "api.orders.fetch"))"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_chunk_real_completion_respects_active_major_mode_punctuation_syntax()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_real_completion_respects_active_major_mode_punctuation_syntax",
        r##"(save-window-excursion
                          (mapcar
                           (lambda (mode)
                             (with-temp-buffer
                               (switch-to-buffer
                                (current-buffer))
                               (funcall mode)
                               (let ((ac-use-comphist nil)
                                     (ac-use-quick-help nil)
                                     (ac-auto-show-menu t)
                                     (ac-expand-on-auto-complete nil)
                                     (ac-sources
                                      '(ac-source-chunk-list))
                                     (ac-chunk-list
                                      '("namespace..alpha"
                                        "namespace..beta"
                                        "namespace.alpha")))
                                 (unwind-protect
                                     (progn
                                       (auto-complete-mode 1)
                                       (insert "namespace..a")
                                       (let ((started
                                              (auto-complete)))
                                         (list
                                          mode
                                          started
                                          ac-prefix
                                          (and ac-candidates
                                               (mapcar
                                                #'substring-no-properties
                                                ac-candidates))
                                          (buffer-substring-no-properties
                                           (line-beginning-position)
                                           (line-end-position)))))
                                   (auto-complete-mode -1)))))
                           '(fundamental-mode
                             emacs-lisp-mode
                             python-mode)))"##,
        expect![[
            r#"OK ((fundamental-mode nil nil nil "namespace..a") (emacs-lisp-mode t nil nil "namespace..alpha") (python-mode nil nil nil "namespace..a"))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_chunk_real_menu_navigates_and_completes_a_python_attribute(),
        auto_complete_chunk_real_menu_incrementally_recomputes_after_attribute_typing(),
        auto_complete_chunk_real_command_completes_from_its_declared_source_only(),
        auto_complete_chunk_dictionary_swap_drives_a_real_completion_session(),
        auto_complete_chunk_two_live_buffers_keep_independent_dictionaries_and_completion_results(),
        auto_complete_chunk_real_completion_respects_active_major_mode_punctuation_syntax(),
    ]
}
