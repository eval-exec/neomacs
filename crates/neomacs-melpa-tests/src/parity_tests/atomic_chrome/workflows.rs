use expect_test::expect;

use super::ParityBatchCase;

fn atomic_chrome_edit_mode_lifecycle_installs_local_hooks_idempotently_and_leaves_them_on_disable()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_edit_mode_lifecycle_installs_local_hooks_idempotently_and_leaves_them_on_disable",
        r##"(with-temp-buffer
          (setq-local
           kill-buffer-hook nil)
          (setq-local
           post-command-hook nil)
          (let ((atomic-chrome-enable-auto-update
                 t)
                events
                snapshots)
            (let ((atomic-chrome-edit-mode-hook
                   (list
                    (lambda ()
                      (push
                       (list
                        'mode-hook
                        atomic-chrome-edit-mode)
                       events)))))
              (dolist
                  (argument
                   '(1 1 -1 nil 0 2))
                (push
                 (list
                  argument
                  (atomic-chrome-edit-mode
                   argument)
                  atomic-chrome-edit-mode
                  (copy-sequence
                   kill-buffer-hook)
                  (copy-sequence
                   post-command-hook)
                  (local-variable-p
                   'kill-buffer-hook)
                  (local-variable-p
                   'post-command-hook))
                 snapshots))
              (list
               (nreverse snapshots)
               (nreverse events)
               (cl-count
                'atomic-chrome-close-connection
                kill-buffer-hook)
               (cl-count
                'atomic-chrome-send-buffer-text
                post-command-hook)))))"##,
        expect![
            "OK (((1 t t (atomic-chrome-close-connection) (atomic-chrome-send-buffer-text) t t) (1 t t (atomic-chrome-close-connection) (atomic-chrome-send-buffer-text) t t) (-1 nil nil (atomic-chrome-close-connection) (atomic-chrome-send-buffer-text) t t) (nil t t (atomic-chrome-close-connection) (atomic-chrome-send-buffer-text) t t) (0 nil nil (atomic-chrome-close-connection) (atomic-chrome-send-buffer-text) t t) (2 t t (atomic-chrome-close-connection) (atomic-chrome-send-buffer-text) t t)) ((mode-hook t) (mode-hook t) (mode-hook nil) (mode-hook t) (mode-hook nil) (mode-hook t)) 1 1)"
        ],
    )
}

fn atomic_chrome_edit_mode_with_auto_update_disabled_installs_only_close_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_edit_mode_with_auto_update_disabled_installs_only_close_hook",
        r##"(with-temp-buffer
          (setq-local
           kill-buffer-hook nil)
          (setq-local
           post-command-hook nil)
          (let ((atomic-chrome-enable-auto-update
                 nil))
            (list
             (atomic-chrome-edit-mode 1)
             atomic-chrome-edit-mode
             (copy-sequence
              kill-buffer-hook)
             (copy-sequence
              post-command-hook)
             (atomic-chrome-edit-mode -1)
             atomic-chrome-edit-mode
             (copy-sequence
              kill-buffer-hook)
             (copy-sequence
              post-command-hook))))"##,
        expect![
            "OK (t t (atomic-chrome-close-connection) nil nil nil (atomic-chrome-close-connection) nil)"
        ],
    )
}

fn atomic_chrome_turn_on_edit_mode_checks_table_membership_and_preserves_unrelated_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_turn_on_edit_mode_checks_table_membership_and_preserves_unrelated_buffer",
        r##"(let ((registered
                (generate-new-buffer
                 " *atomic-turn-on-registered*"))
               (unrelated
                (generate-new-buffer
                 " *atomic-turn-on-unrelated*"))
               (atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal)))
          (unwind-protect
              (progn
                (puthash
                 registered
                 '(:socket nil)
                 atomic-chrome-buffer-table)
                (list
                 (with-current-buffer registered
                   (list
                    (atomic-chrome-turn-on-edit-mode)
                    atomic-chrome-edit-mode
                    (and
                     (memq
                      'atomic-chrome-close-connection
                      kill-buffer-hook)
                     t)))
                 (with-current-buffer unrelated
                   (list
                    (atomic-chrome-turn-on-edit-mode)
                    atomic-chrome-edit-mode
                    (and
                     (memq
                      'atomic-chrome-close-connection
                      kill-buffer-hook)
                     t)))))
            (atomic-chrome-test-kill-buffer
             registered)
            (atomic-chrome-test-kill-buffer
             unrelated)))"##,
        expect!["OK ((t t t) (nil nil nil))"],
    )
}

fn global_atomic_chrome_edit_mode_enables_only_registered_live_buffers_and_disables_mode()
-> ParityBatchCase {
    ParityBatchCase::value(
        "global_atomic_chrome_edit_mode_enables_only_registered_live_buffers_and_disables_mode",
        r##"(let ((registered
                (generate-new-buffer
                 " *atomic-global-registered*"))
               (unrelated
                (generate-new-buffer
                 " *atomic-global-unrelated*"))
               (atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal))
               enabled
               disabled)
          (unwind-protect
              (progn
                (puthash
                 registered
                 '(:socket nil)
                 atomic-chrome-buffer-table)
                (global-atomic-chrome-edit-mode 1)
                (setq enabled
                      (list
                       global-atomic-chrome-edit-mode
                       (with-current-buffer registered
                         (list
                          atomic-chrome-edit-mode
                          (and
                           (memq
                            'atomic-chrome-close-connection
                            kill-buffer-hook)
                           t)
                          (and
                           (memq
                            'atomic-chrome-send-buffer-text
                            post-command-hook)
                           t)))
                       (with-current-buffer unrelated
                         atomic-chrome-edit-mode)))
                (global-atomic-chrome-edit-mode 0)
                (setq disabled
                      (list
                       global-atomic-chrome-edit-mode
                       (with-current-buffer registered
                         (list
                          atomic-chrome-edit-mode
                          (and
                           (memq
                            'atomic-chrome-close-connection
                            kill-buffer-hook)
                           t)
                          (and
                           (memq
                            'atomic-chrome-send-buffer-text
                            post-command-hook)
                           t)))
                       (with-current-buffer unrelated
                         atomic-chrome-edit-mode)))
                (list enabled disabled))
            (when global-atomic-chrome-edit-mode
              (global-atomic-chrome-edit-mode 0))
            (atomic-chrome-test-kill-buffer
             registered)
            (atomic-chrome-test-kill-buffer
             unrelated)))"##,
        expect!["OK ((t (t t t) nil) (nil (nil t t) nil))"],
    )
}

fn atomic_chrome_close_edit_buffer_runs_done_hook_then_frame_and_split_window_cleanup()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_close_edit_buffer_runs_done_hook_then_frame_and_split_window_cleanup",
        r##"(let ((buffer
                (generate-new-buffer
                 " *atomic-close-split*"))
               (atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal))
               (atomic-chrome-buffer-open-style
                'split)
               events)
          (unwind-protect
              (progn
                (with-current-buffer buffer
                  (insert "prefix editable suffix")
                  (narrow-to-region 8 16)
                  (setq-local
                   atomic-chrome-edit-done-hook
                   (list
                    (lambda ()
                      (push
                       (list
                        'done
                        (point-min)
                        (point-max)
                        (buffer-string))
                       events)))))
                (puthash
                 buffer
                 '(:socket :edit-frame)
                 atomic-chrome-buffer-table)
                (cl-letf
                    (((symbol-function 'get-buffer-window)
                      (lambda (target)
                        (push
                         (list
                          'window
                          (buffer-name target))
                         events)
                        :edit-window))
                     ((symbol-function 'delete-frame)
                      (lambda (frame)
                        (push
                         (list 'delete-frame frame)
                         events)
                        :deleted))
                     ((symbol-function 'quit-window)
                      (lambda (kill window)
                        (push
                         (list
                          'quit
                          kill
                          window
                          (buffer-live-p buffer))
                         events)
                        :quit))
                     ((symbol-function 'kill-buffer)
                      (lambda (target)
                        (push
                         (list
                          'unexpected-kill
                          target)
                         events))))
                  (list
                   (atomic-chrome-close-edit-buffer
                    buffer)
                   (buffer-live-p buffer)
                   (nreverse events))))
            (atomic-chrome-test-kill-buffer
             buffer)))"##,
        expect![[
            r#"OK (:quit t ((window " *atomic-close-split*") (done 8 16 "editable") (delete-frame :edit-frame) (quit t :edit-window t)))"#
        ]],
    )
}

fn atomic_chrome_atomic_protocol_practical_edit_update_send_and_disconnect_workflow()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_atomic_protocol_practical_edit_update_send_and_disconnect_workflow",
        r##"(let ((atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal))
               (atomic-chrome-server-ghost-text
                :ghost-server)
               (atomic-chrome-buffer-open-style
                'full)
               (atomic-chrome-enable-auto-update
                t)
               (socket
                (atomic-chrome-test-socket
                 'browser-socket
                 :atomic-server))
               events
               edit-buffer)
          (unwind-protect
              (cl-letf
                  (((symbol-function
                     'atomic-chrome-show-edit-buffer)
                    (lambda (buffer title)
                      (push
                       (list
                        'show
                        (buffer-name buffer)
                        title)
                       events)
                      nil))
                   ((symbol-function
                     'websocket-send-text)
                    (lambda (target text)
                      (push
                       (list
                        'send
                        (atomic-chrome-test-socket-name
                         target)
                        text)
                       events)
                      :sent))
                   ((symbol-function 'websocket-close)
                    (lambda (target)
                      (push
                       (list
                        'socket-close
                        (atomic-chrome-test-socket-name
                         target))
                       events)
                      :closed)))
                (let ((atomic-chrome-url-major-mode-alist
                       '(("code\\.example"
                          . emacs-lisp-mode))))
                  (atomic-chrome-on-message
                   socket
                   (atomic-chrome-test-frame
                    "{\"type\":\"register\",\"payload\":{\"url\":\"https://code.example/file.el\",\"title\":\"Browser editor\",\"text\":\"(+ 1 2)\"}}")))
                (maphash
                 (lambda (buffer _value)
                   (setq edit-buffer buffer))
                atomic-chrome-buffer-table)
                (with-current-buffer edit-buffer
                  (setq-local
                   kill-buffer-hook nil)
                  (setq-local
                   post-command-hook nil)
                  (atomic-chrome-edit-mode 1)
                  (goto-char
                   (point-max))
                  (insert "\n;; local")
                  (run-hooks
                   'post-command-hook))
                (let ((after-send
                       (atomic-chrome-test-buffer-state
                        edit-buffer)))
                  (atomic-chrome-on-message
                   socket
                   (atomic-chrome-test-frame
                    "{\"type\":\"updateText\",\"payload\":{\"text\":\"(+ 20 22)\"}}"))
                  (let ((after-update
                         (atomic-chrome-test-buffer-state
                          edit-buffer)))
                    (atomic-chrome-on-close
                     socket)
                    (list
                     after-send
                     after-update
                     (buffer-live-p
                      edit-buffer)
                     (hash-table-count
                      atomic-chrome-buffer-table)
                     (nreverse events)))))
            (atomic-chrome-test-kill-buffer
             edit-buffer)))"##,
        expect![[
            r#"OK (("Browser editor" "(+ 1 2)\n;; local" emacs-lisp-mode t t t nil) ("Browser editor" "(+ 20 22)" emacs-lisp-mode t t t t) nil 0 ((show "Browser editor" "Browser editor") (send browser-socket "{\"type\":\"updateText\",\"payload\":{\"text\":\"(+ 1 2)\\n;; local\"}}") (socket-close browser-socket)))"#
        ]],
    )
}

fn atomic_chrome_ghost_text_practical_create_bidirectional_update_and_send_workflow()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_ghost_text_practical_create_bidirectional_update_and_send_workflow",
        r##"(let ((atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal))
               (atomic-chrome-server-ghost-text
                :ghost-server)
               (atomic-chrome-enable-bidirectional-edit
                t)
               (socket
                (atomic-chrome-test-socket
                 'ghost-socket
                 :ghost-server))
               events
               edit-buffer)
          (unwind-protect
              (cl-letf
                  (((symbol-function
                     'atomic-chrome-show-edit-buffer)
                    (lambda (buffer title)
                      (push
                       (list
                        'show
                        (buffer-name buffer)
                        title)
                       events)
                      :ghost-frame))
                   ((symbol-function
                     'websocket-send-text)
                    (lambda (target text)
                      (push
                       (list
                        'send
                        (atomic-chrome-test-socket-name
                         target)
                        text)
                       events)
                      :sent)))
                (atomic-chrome-on-message
                 socket
                 (atomic-chrome-test-frame
                  "{\"url\":\"ghost.example\",\"title\":\"Ghost editor\",\"text\":\"draft\"}"))
                (setq edit-buffer
                      (atomic-chrome-get-buffer-by-socket
                       socket))
                (let ((created
                       (atomic-chrome-test-buffer-state
                        edit-buffer)))
                  (atomic-chrome-on-message
                   socket
                   (atomic-chrome-test-frame
                    "{\"url\":\"ghost.example\",\"title\":\"Ghost editor\",\"text\":\"browser revision\"}"))
                  (with-current-buffer edit-buffer
                    (goto-char
                     (point-max))
                    (insert " + emacs")
                    (atomic-chrome-send-buffer-text))
                  (list
                   created
                   (atomic-chrome-test-buffer-state
                    edit-buffer)
                   (atomic-chrome-test-buffer-table-snapshot)
                   (nreverse events))))
            (atomic-chrome-test-kill-buffer
             edit-buffer)))"##,
        expect![[
            r#"OK (("Ghost editor" "draft" text-mode nil nil nil t) ("Ghost editor" "browser revision + emacs" text-mode nil nil nil nil) (("Ghost editor" (ghost-socket :ghost-server) :ghost-frame)) ((show "Ghost editor" "Ghost editor") (send ghost-socket "{\"text\":\"browser revision + emacs\"}")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atomic_chrome_edit_mode_lifecycle_installs_local_hooks_idempotently_and_leaves_them_on_disable(),
        atomic_chrome_edit_mode_with_auto_update_disabled_installs_only_close_hook(),
        atomic_chrome_turn_on_edit_mode_checks_table_membership_and_preserves_unrelated_buffer(),
        global_atomic_chrome_edit_mode_enables_only_registered_live_buffers_and_disables_mode(),
        atomic_chrome_close_edit_buffer_runs_done_hook_then_frame_and_split_window_cleanup(),
        atomic_chrome_atomic_protocol_practical_edit_update_send_and_disconnect_workflow(),
        atomic_chrome_ghost_text_practical_create_bidirectional_update_and_send_workflow(),
    ]
}
