use expect_test::expect;

use super::ParityBatchCase;

fn atomic_chrome_set_major_mode_selects_first_matching_url_rule_and_falls_back_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_set_major_mode_selects_first_matching_url_rule_and_falls_back_exactly",
        r##"(let ((atomic-chrome-url-major-mode-alist
                '(("github\\.com/repo"
                   . emacs-lisp-mode)
                  ("github\\.com"
                   . text-mode)
                  ("example"
                   . fundamental-mode)))
               (atomic-chrome-default-major-mode
                'special-mode))
          (mapcar
           (lambda (url)
             (with-temp-buffer
               (list
                url
                (atomic-chrome-set-major-mode
                 url)
                major-mode
                mode-name)))
           '("https://github.com/repo/file.el"
             "https://github.com/issues"
             "https://example.test/"
             "https://unmatched.test/"
             ""
             nil)))"##,
        expect![[
            r#"OK (("https://github.com/repo/file.el" nil emacs-lisp-mode ("Elisp" (lexical-binding (:propertize "/l" help-echo "Using lexical-binding mode") (:propertize "/d" help-echo "Using old dynamic scoping mode\nmouse-1: Enable lexical-binding mode" face warning mouse-face mode-line-highlight local-map (keymap (mode-line keymap (mouse-1 . elisp-enable-lexical-binding))))))) ("https://github.com/issues" nil text-mode "Text") ("https://example.test/" nil fundamental-mode "Fundamental") ("https://unmatched.test/" nil special-mode "Special") ("" nil special-mode "Special") (nil nil special-mode "Special"))"#
        ]],
    )
}

fn atomic_chrome_set_major_mode_invokes_selected_function_once_and_propagates_invalid_rules()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_set_major_mode_invokes_selected_function_once_and_propagates_invalid_rules",
        r##"(let (events)
          (cl-labels
              ((selected-mode ()
                 (push
                  (list
                   'selected
                   (buffer-name)
                   major-mode)
                  events)
                 (setq major-mode
                       'selected-mode)
                 :selected-result)
               (fallback-mode ()
                 (push
                  (list
                   'fallback
                   (buffer-name)
                   major-mode)
                  events)
                 (setq major-mode
                       'fallback-mode)
                 :fallback-result))
            (let ((atomic-chrome-url-major-mode-alist
                   (list
                    (cons
                     "match"
                     #'selected-mode)))
                  (atomic-chrome-default-major-mode
                   #'fallback-mode))
              (list
               (with-temp-buffer
                 (list
                  (atomic-chrome-set-major-mode
                   "match.example")
                  major-mode))
               (with-temp-buffer
                 (list
                  (atomic-chrome-set-major-mode
                   "other.example")
                  major-mode))
               (let ((atomic-chrome-url-major-mode-alist
                      '(("[" . selected-mode))))
                 (with-temp-buffer
                   (atomic-chrome-test-error-data
                    (lambda ()
                      (atomic-chrome-set-major-mode
                       "anything")))))
               (nreverse events)))))"##,
        expect![[
            r#"OK ((:selected-result selected-mode) (:fallback-result fallback-mode) (:error invalid-regexp ("Unmatched [ or [^")) ((selected " *temp*" fundamental-mode) (fallback " *temp*" fundamental-mode)))"#
        ]],
    )
}

fn atomic_chrome_show_edit_buffer_full_and_split_styles_call_exact_window_operations()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_show_edit_buffer_full_and_split_styles_call_exact_window_operations",
        r##"(let ((buffer
                (generate-new-buffer
                 " *atomic-show*"))
               events)
          (unwind-protect
              (cl-letf
                  (((symbol-function 'pop-to-buffer)
                    (lambda (target)
                      (push
                       (list
                        'pop
                        (buffer-name target))
                       events)
                      :pop-window))
                   ((symbol-function 'switch-to-buffer)
                    (lambda (target)
                      (push
                       (list
                        'switch
                        (buffer-name target))
                       events)
                      target))
                   ((symbol-function 'raise-frame)
                    (lambda (frame)
                      (push
                       (list 'raise frame)
                       events)
                      frame))
                   ((symbol-function 'selected-window)
                    (lambda ()
                      :selected-window))
                   ((symbol-function 'window-frame)
                    (lambda (window)
                      (push
                       (list 'window-frame window)
                       events)
                      :selected-frame))
                   ((symbol-function
                     'select-frame-set-input-focus)
                    (lambda (frame)
                      (push
                       (list 'focus frame)
                       events)
                      frame)))
                (let ((atomic-chrome-buffer-open-style
                       'full))
                  (push
                   (list
                    :full-return
                    (atomic-chrome-show-edit-buffer
                     buffer
                     "Full title"))
                   events))
                (let ((atomic-chrome-buffer-open-style
                       'split))
                  (push
                   (list
                    :split-return
                    (atomic-chrome-show-edit-buffer
                     buffer
                     "Split title"))
                   events))
                (nreverse events))
            (atomic-chrome-test-kill-buffer buffer)))"##,
        expect![[
            r#"OK ((switch " *atomic-show*") (raise nil) (window-frame :selected-window) (focus :selected-frame) (:full-return nil) (pop " *atomic-show*") (raise nil) (window-frame :selected-window) (focus :selected-frame) (:split-return nil))"#
        ]],
    )
}

fn atomic_chrome_show_edit_buffer_frame_style_selects_platform_specific_frame_constructor()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_show_edit_buffer_frame_style_selects_platform_specific_frame_constructor",
        r##"(let ((buffer
                (generate-new-buffer
                 " *atomic-frame-show*"))
               (atomic-chrome-buffer-frame-width
                101)
               (atomic-chrome-buffer-frame-height
                37)
               snapshots)
          (unwind-protect
              (dolist
                  (case
                   '((pgtk nil nil)
                     (x "wayland-1" ":8")
                     (x ":7" ":8")
                     (ns nil nil)
                     (mac nil nil)
                     (w32 nil nil)
                     (nil nil nil)))
                (let ((window-system
                       (nth 0 case))
                      (x-display-name
                       (nth 1 case))
                      (display
                       (nth 2 case))
                      (atomic-chrome-buffer-open-style
                       'frame)
                      events)
                  (cl-letf
                      (((symbol-function 'getenv)
                        (lambda (name)
                          (and
                           (equal name "DISPLAY")
                           display)))
                       ((symbol-function 'make-frame)
                        (lambda (parameters)
                          (push
                           (list
                            'make-frame
                            parameters)
                           events)
                          :made-frame))
                       ((symbol-function
                         'make-frame-on-display)
                        (lambda (target parameters)
                          (push
                           (list
                            'make-on
                            target
                            parameters)
                           events)
                          :made-on-display))
                       ((symbol-function 'select-frame)
                        (lambda (frame)
                          (push
                           (list 'select frame)
                           events)
                          frame))
                       ((symbol-function 'switch-to-buffer)
                        (lambda (target)
                          (push
                           (list
                            'switch
                            (buffer-name target))
                           events)
                          target))
                       ((symbol-function 'raise-frame)
                        (lambda (frame)
                          (push
                           (list 'raise frame)
                           events)
                          frame))
                       ((symbol-function 'selected-window)
                        (lambda ()
                          :selected-window))
                       ((symbol-function 'window-frame)
                        (lambda (window)
                          (push
                           (list
                            'window-frame
                            window)
                           events)
                          :active-frame))
                       ((symbol-function
                         'select-frame-set-input-focus)
                        (lambda (frame)
                          (push
                           (list 'focus frame)
                           events)
                          frame)))
                    (let ((result
                           (atomic-chrome-show-edit-buffer
                            buffer
                            "Editor")))
                      (push
                       (list
                        case
                        result
                        (nreverse events))
                       snapshots)))))
            (atomic-chrome-test-kill-buffer buffer))
          (nreverse snapshots))"##,
        expect![[
            r#"OK (((pgtk nil nil) :made-frame ((make-frame ((name . "Atomic Chrome: Editor") (width . 101) (height . 37))) (select :made-frame) (switch " *atomic-frame-show*") (raise :made-frame) (window-frame :selected-window) (focus :active-frame))) ((x "wayland-1" ":8") :made-frame ((make-frame ((name . "Atomic Chrome: Editor") (width . 101) (height . 37))) (select :made-frame) (switch " *atomic-frame-show*") (raise :made-frame) (window-frame :selected-window) (focus :active-frame))) ((x ":7" ":8") :made-on-display ((make-on ":8" ((name . "Atomic Chrome: Editor") (width . 101) (height . 37))) (select :made-on-display) (switch " *atomic-frame-show*") (raise :made-on-display) (window-frame :selected-window) (focus :active-frame))) ((ns nil nil) :made-frame ((make-frame ((name . "Atomic Chrome: Editor") (width . 101) (height . 37))) (select :made-frame) (switch " *atomic-frame-show*") (raise :made-frame) (window-frame :selected-window) (focus :active-frame))) ((mac nil nil) :made-frame ((make-frame ((name . "Atomic Chrome: Editor") (width . 101) (height . 37))) (select :made-frame) (switch " *atomic-frame-show*") (raise :made-frame) (window-frame :selected-window) (focus :active-frame))) ((w32 nil nil) :made-on-display ((make-on "w32" ((name . "Atomic Chrome: Editor") (width . 101) (height . 37))) (select :made-on-display) (switch " *atomic-frame-show*") (raise :made-on-display) (window-frame :selected-window) (focus :active-frame))) ((nil nil nil) :made-frame ((make-frame ((name . "Atomic Chrome: Editor") (width . 101) (height . 37))) (select :made-frame) (switch " *atomic-frame-show*") (raise :made-frame) (window-frame :selected-window) (focus :active-frame))))"#
        ]],
    )
}

fn atomic_chrome_create_buffer_assigns_unique_title_mode_text_frame_and_table_entry()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_create_buffer_assigns_unique_title_mode_text_frame_and_table_entry",
        r##"(let ((atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal))
               (atomic-chrome-url-major-mode-alist
                '(("code"
                   . emacs-lisp-mode)))
               (atomic-chrome-default-major-mode
                'text-mode)
               events
               buffers)
          (unwind-protect
              (cl-letf
                  (((symbol-function
                     'atomic-chrome-show-edit-buffer)
                    (lambda (buffer title)
                      (push
                       (list
                        'show
                        (buffer-name buffer)
                        title
                        (gethash
                         buffer
                         atomic-chrome-buffer-table))
                       events)
                      (intern
                       (concat
                        ":frame-"
                        (if
                            (string-empty-p title)
                            "empty"
                          title))))))
                (dolist
                    (spec
                     '((:socket-a
                        "https://code.example/a"
                        "Editor"
                        "(message \"one\")")
                       (:socket-b
                        "https://plain.example/b"
                        "Editor"
                        "plain text")
                       (:socket-c
                        nil
                        ""
                        "")))
                  (let ((before
                         (buffer-list)))
                    (atomic-chrome-create-buffer
                     (nth 0 spec)
                     (nth 1 spec)
                     (nth 2 spec)
                     (nth 3 spec))
                    (let ((created
                           (car
                            (seq-difference
                             (buffer-list)
                             before))))
                      (push created buffers))))
                (list
                 (mapcar
                  #'atomic-chrome-test-buffer-state
                  (nreverse
                   (copy-sequence buffers)))
                 (nreverse events)
                 (atomic-chrome-test-buffer-table-snapshot)))
            (mapc
             #'atomic-chrome-test-kill-buffer
             buffers)))"##,
        expect![[
            r#"OK ((("Editor" "(message \"one\")" emacs-lisp-mode nil nil nil t) ("Editor<2>" "plain text" text-mode nil nil nil t) ("No title" "" text-mode nil nil nil nil)) ((show "Editor" "Editor" nil) (show "Editor<2>" "Editor" nil) (show "No title" "" nil)) (("Editor" :socket-a :frame-Editor) ("Editor<2>" :socket-b :frame-Editor) ("No title" :socket-c :frame-empty)))"#
        ]],
    )
}

fn atomic_chrome_update_buffer_replaces_contents_preserves_table_and_handles_missing_socket()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_update_buffer_replaces_contents_preserves_table_and_handles_missing_socket",
        r##"(let ((buffer
                (generate-new-buffer
                 " *atomic-update*"))
               (atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal)))
          (unwind-protect
              (progn
                (with-current-buffer buffer
                  (insert "old content")
                  (goto-char 5)
                  (set-buffer-modified-p nil))
                (puthash
                 buffer
                 '(:socket nil)
                 atomic-chrome-buffer-table)
                (list
                 (atomic-chrome-update-buffer
                  :socket
                  "new\ncontent")
                 (with-current-buffer buffer
                   (list
                    (buffer-string)
                    (point)
                    (buffer-modified-p)))
                 (atomic-chrome-update-buffer
                  :missing
                  "ignored")
                 (with-current-buffer buffer
                   (buffer-string))
                 (atomic-chrome-test-buffer-table-snapshot)))
            (atomic-chrome-test-kill-buffer buffer)))"##,
        expect![[
            r#"OK (nil ("new\ncontent" 12 t) nil "new\ncontent" ((" *atomic-update*" :socket nil)))"#
        ]],
    )
}

fn atomic_chrome_update_buffer_propagates_read_only_failure_without_mutating_old_text()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_update_buffer_propagates_read_only_failure_without_mutating_old_text",
        r##"(let ((buffer
                (generate-new-buffer
                 " *atomic-update-read-only*"))
               (atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal)))
          (unwind-protect
              (progn
                (with-current-buffer buffer
                  (insert "locked")
                  (setq-local
                   buffer-read-only t)
                  (set-buffer-modified-p nil))
                (puthash
                 buffer
                 '(:socket nil)
                 atomic-chrome-buffer-table)
                (list
                 (atomic-chrome-test-error-data
                  (lambda ()
                    (atomic-chrome-update-buffer
                     :socket
                     "replacement")))
                 (with-current-buffer buffer
                   (list
                    (buffer-string)
                    (buffer-modified-p)
                    buffer-read-only))
                 (atomic-chrome-test-buffer-table-snapshot)))
            (atomic-chrome-test-kill-buffer buffer)))"##,
        expect![[
            r#"OK ((:error buffer-read-only ((:buffer nil))) ("locked" nil t) ((" *atomic-update-read-only*" :socket nil)))"#
        ]],
    )
}

fn atomic_chrome_close_current_buffer_obeys_modified_confirmation_before_delegating()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_close_current_buffer_obeys_modified_confirmation_before_delegating",
        r##"(let (events)
          (cl-letf
              (((symbol-function 'yes-or-no-p)
                (lambda (prompt)
                  (push
                   (list 'prompt prompt)
                   events)
                  nil))
               ((symbol-function
                 'atomic-chrome-close-edit-buffer)
                (lambda (buffer)
                  (push
                   (list
                    'close
                    (buffer-name buffer))
                   events)
                  :closed)))
            (list
             (with-temp-buffer
               (set-buffer-modified-p nil)
               (atomic-chrome-close-current-buffer))
             (with-temp-buffer
               (insert "modified")
               (atomic-chrome-close-current-buffer))
             (let ((decline-events
                    (nreverse events)))
               (setq events nil)
               (cl-letf
                   (((symbol-function 'yes-or-no-p)
                     (lambda (prompt)
                       (push
                        (list 'prompt prompt)
                        events)
                       t)))
                 (list
                  decline-events
                  (with-temp-buffer
                    (insert "modified")
                    (atomic-chrome-close-current-buffer))
                  (nreverse events)))))))"##,
        expect![[
            r#"OK (:closed nil (((close " *temp*") (prompt "Buffer has not been saved, close anyway? ")) :closed ((prompt "Buffer has not been saved, close anyway? ") (close " *temp*"))))"#
        ]],
    )
}

pub(super) fn buffers_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atomic_chrome_set_major_mode_selects_first_matching_url_rule_and_falls_back_exactly(),
        atomic_chrome_set_major_mode_invokes_selected_function_once_and_propagates_invalid_rules(),
        atomic_chrome_show_edit_buffer_full_and_split_styles_call_exact_window_operations(),
        atomic_chrome_show_edit_buffer_frame_style_selects_platform_specific_frame_constructor(),
        atomic_chrome_create_buffer_assigns_unique_title_mode_text_frame_and_table_entry(),
        atomic_chrome_update_buffer_replaces_contents_preserves_table_and_handles_missing_socket(),
        atomic_chrome_update_buffer_propagates_read_only_failure_without_mutating_old_text(),
        atomic_chrome_close_current_buffer_obeys_modified_confirmation_before_delegating(),
    ]
}
