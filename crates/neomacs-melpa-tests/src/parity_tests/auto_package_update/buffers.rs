use expect_test::expect;

use super::ParityBatchCase;

fn auto_package_update_visible_write_creates_real_read_only_results_buffer_and_mode()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_visible_write_creates_real_read_only_results_buffer_and_mode",
        r##"(let ((name
                                " *apu-visible-results*"))
                           (unwind-protect
                               (save-window-excursion
                                 (let ((result
                                        (apu--write-buffer
                                         "alpha up to date.\nbeta failed"
                                         name
                                         nil)))
                                   (with-current-buffer name
                                     (list
                                      result
                                      (buffer-name)
                                      (buffer-string)
                                      buffer-read-only
                                      auto-package-update-minor-mode
                                      (key-binding (kbd "q"))
                                      (buffer-modified-p)
                                      (eq
                                       (current-buffer)
                                       (get-buffer name))))))
                             (auto-package-update-test-kill-buffers
                              name)))"##,
        expect![[
            r#"OK (t " *apu-visible-results*" "alpha up to date.\nbeta failed" t t quit-window t t)"#
        ]],
    )
}

fn auto_package_update_write_replaces_existing_read_only_contents_without_duplicate_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_write_replaces_existing_read_only_contents_without_duplicate_state",
        r##"(let ((name
                                " *apu-overwrite-results*"))
                           (unwind-protect
                               (progn
                                 (with-current-buffer
                                     (get-buffer-create name)
                                   (insert "stale contents")
                                   (read-only-mode 1)
                                   (auto-package-update-minor-mode 1))
                                 (save-window-excursion
                                   (apu--write-buffer
                                    "fresh\nreport"
                                    name))
                                 (with-current-buffer name
                                   (list
                                    (buffer-string)
                                    buffer-read-only
                                    auto-package-update-minor-mode
                                    (key-binding (kbd "q"))
                                    (buffer-size)
                                    (local-variable-p
                                     'auto-package-update-minor-mode))))
                             (auto-package-update-test-kill-buffers
                              name)))"##,
        expect![[r#"OK ("fresh\nreport" t t quit-window 12 t)"#]],
    )
}

fn auto_package_update_hidden_write_avoids_popup_and_buries_named_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_hidden_write_avoids_popup_and_buries_named_buffer",
        r##"(let
                             ((name
                               " *apu-hidden-results*")
                              events)
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'pop-to-buffer)
                                     (lambda (&rest arguments)
                                       (error
                                        "must not pop: %S"
                                        arguments)))
                                    ((symbol-function
                                      'bury-buffer)
                                     (lambda (&optional buffer)
                                       (push
                                        (list
                                         :bury
                                         (buffer-name
                                          (or
                                           (and
                                            (bufferp buffer)
                                            buffer)
                                           (current-buffer))))
                                        events)
                                       :buried)))
                                 (let ((result
                                        (apu--write-buffer
                                         "quiet report"
                                         name
                                         t)))
                                   (with-current-buffer name
                                     (list
                                      result
                                      (nreverse events)
                                      (buffer-string)
                                      buffer-read-only
                                      auto-package-update-minor-mode))))
                             (auto-package-update-test-kill-buffers
                              name)))"##,
        expect![[r#"OK (t ((:bury " *apu-hidden-results*")) "quiet report" t t)"#]],
    )
}

fn auto_package_update_hide_preview_selects_and_kills_existing_preview_window_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_hide_preview_selects_and_kills_existing_preview_window_buffer",
        r##"(let
                             ((auto-package-preview-buffer-name
                               " *apu-hide-preview*")
                              events)
                           (get-buffer-create
                            auto-package-preview-buffer-name)
                           (cl-letf
                               (((symbol-function
                                  'kill-buffer-and-window)
                                 (lambda ()
                                   (push
                                    (list
                                     :kill
                                     (buffer-name)
                                     (eq
                                      (current-buffer)
                                      (get-buffer
                                       auto-package-preview-buffer-name)))
                                    events)
                                   (kill-buffer
                                    (current-buffer))
                                   :killed)))
                             (list
                              (apu--hide-preview)
                              (nreverse events)
                              (get-buffer
                               auto-package-preview-buffer-name))))"##,
        expect![[r#"OK (:killed ((:kill " *apu-hide-preview*" t)) nil)"#]],
    )
}

pub(super) fn buffers_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_package_update_visible_write_creates_real_read_only_results_buffer_and_mode(),
        auto_package_update_write_replaces_existing_read_only_contents_without_duplicate_state(),
        auto_package_update_hidden_write_avoids_popup_and_buries_named_buffer(),
        auto_package_update_hide_preview_selects_and_kills_existing_preview_window_buffer(),
    ]
}
