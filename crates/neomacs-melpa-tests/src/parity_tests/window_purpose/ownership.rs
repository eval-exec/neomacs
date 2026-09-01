use expect_test::expect;

use super::ParityBatchCase;

fn changing_window_ownership_prefers_real_buffers_and_uses_a_dummy_only_when_needed()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-window-purpose-test-with-configuration
    nil
    nil
    '(("wp-unit-tests" . tests)
      ("wp-editor" . edit))
    nil
  (let ((tests (get-buffer-create "wp-unit-tests"))
        (editor (get-buffer-create "wp-editor"))
        dummy
        result)
    (save-window-excursion
      (unwind-protect
          (progn
            (delete-other-windows)
            (set-window-buffer nil editor)
            (purpose-set-window-purpose 'tests)
            (let ((real-buffer
                   (list :buffer (buffer-name (window-buffer))
                         :purpose (purpose-window-purpose)
                         :purpose-dedicated
                         (and (purpose-window-purpose-dedicated-p) t)
                         :buffer-dedicated (window-dedicated-p)
                         :modeline (purpose--modeline-string))))
              (purpose-set-window-purpose 'docs)
              (setq dummy (window-buffer))
              (let ((dummy-buffer
                     (list :buffer (buffer-name dummy)
                           :purpose (purpose-window-purpose)
                           :purpose-dedicated
                           (and (purpose-window-purpose-dedicated-p) t)
                           :buffer-dedicated (window-dedicated-p)
                           :modeline (purpose--modeline-string)
                           :known-doc-buffers
                           (sort (mapcar #'buffer-name
                                         (purpose-buffers-with-purpose 'docs))
                                 #'string<))))
                (purpose-toggle-window-buffer-dedicated)
                (let ((both-owned
                       (list (purpose--modeline-string)
                             (and (purpose-window-purpose-dedicated-p) t)
                             (window-dedicated-p))))
                  (purpose-toggle-window-purpose-dedicated)
                  (let ((buffer-only
                         (list (purpose--modeline-string)
                               (and (purpose-window-purpose-dedicated-p) t)
                               (window-dedicated-p))))
                    (purpose-toggle-window-buffer-dedicated)
                    (purpose-set-window-purpose 'edit t)
                    (setq result
                          (list
                           :real-buffer real-buffer
                           :dummy-buffer dummy-buffer
                           :both-owned both-owned
                           :buffer-only buffer-only
                           :returned-to-edit
                           (list :buffer (buffer-name (window-buffer))
                                 :purpose (purpose-window-purpose)
                                 :purpose-dedicated
                                 (and (purpose-window-purpose-dedicated-p) t)
                                 :buffer-dedicated (window-dedicated-p)
                                 :modeline (purpose--modeline-string))
                           :dummy-still-live (buffer-live-p dummy)))))))
            result)
        (set-window-dedicated-p nil nil)
        (purpose-set-window-purpose-dedicated-p nil nil)
        (neomacs-window-purpose-test-kill-buffers tests editor dummy)))))
"##;
    let expect = expect![[
        r#"OK (:real-buffer (:buffer "wp-unit-tests" :purpose tests :purpose-dedicated t :buffer-dedicated nil :modeline " [tests!]") :dummy-buffer (:buffer "*pu-dummy-docs*" :purpose docs :purpose-dedicated t :buffer-dedicated nil :modeline " [docs!]" :known-doc-buffers ("*pu-dummy-docs*")) :both-owned (" [docs!#]" t t) :buffer-only (" [docs#]" nil t) :returned-to-edit (:buffer "wp-editor" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :modeline " [edit]") :dummy-still-live t)"#
    ]];
    ParityBatchCase::value(
        "changing_window_ownership_prefers_real_buffers_and_uses_a_dummy_only_when_needed",
        elisp_form,
        expect,
    )
}

fn deleting_unowned_panes_preserves_both_purpose_and_buffer_owned_work() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-window-purpose-test-with-configuration
    nil
    nil nil
    '(("^wp-owned-edit$" . edit)
      ("^wp-owned-terminal$" . terminal)
      ("^wp-unowned-" . auxiliary))
  (let ((edit (get-buffer-create "wp-owned-edit"))
        (terminal (get-buffer-create "wp-owned-terminal"))
        (build (get-buffer-create "wp-unowned-build"))
        (notes (get-buffer-create "wp-unowned-notes"))
        result)
    (save-window-excursion
      (unwind-protect
          (progn
            (delete-other-windows)
            (let* ((edit-window (selected-window))
                   (terminal-window (split-window edit-window nil 'right))
                   (build-window (split-window edit-window nil 'below))
                   (notes-window (split-window terminal-window nil 'below)))
              (set-window-buffer edit-window edit)
              (set-window-buffer terminal-window terminal)
              (set-window-buffer build-window build)
              (set-window-buffer notes-window notes)
              (purpose-set-window-purpose-dedicated-p edit-window t)
              (set-window-dedicated-p terminal-window t)
              (select-window notes-window)
              (purpose-delete-non-dedicated-windows)
              (let ((after-first
                     (sort
                      (mapcar
                       (lambda (window)
                         (list (buffer-name (window-buffer window))
                               (purpose-window-purpose window)
                               (and
                                (purpose-window-purpose-dedicated-p window)
                                t)
                               (window-dedicated-p window)
                               (eq window (selected-window))))
                       (window-list nil 'nomini))
                      (lambda (left right)
                        (string< (car left) (car right))))))
                (purpose-set-window-purpose-dedicated-p edit-window nil)
                (purpose-delete-non-dedicated-windows)
                (setq result
                      (list
                       :first-survivors after-first
                       :liveness-after-first
                       (list (window-live-p edit-window)
                             (window-live-p terminal-window)
                             (window-live-p build-window)
                             (window-live-p notes-window))
                       :final-survivors
                       (mapcar
                        (lambda (window)
                          (list (buffer-name (window-buffer window))
                                (purpose-window-purpose window)
                                (and
                                 (purpose-window-purpose-dedicated-p window)
                                 t)
                                (window-dedicated-p window)
                                (eq window (selected-window))))
                        (window-list nil 'nomini))
                       :liveness-after-second
                       (list (window-live-p edit-window)
                             (window-live-p terminal-window)
                             (window-live-p build-window)
                             (window-live-p notes-window))))))
            result)
        (dolist (window (window-list nil 'nomini))
          (set-window-dedicated-p window nil)
          (purpose-set-window-purpose-dedicated-p window nil))
        (neomacs-window-purpose-test-kill-buffers
         edit terminal build notes)))))
"##;
    let expect = expect![[
        r#"OK (:first-survivors (("wp-owned-edit" edit t nil t) ("wp-owned-terminal" terminal nil t nil)) :liveness-after-first (nil t nil nil) :final-survivors (("wp-owned-terminal" terminal nil t t)) :liveness-after-second (nil t nil nil))"#
    ]];
    ParityBatchCase::value(
        "deleting_unowned_panes_preserves_both_purpose_and_buffer_owned_work",
        elisp_form,
        expect,
    )
}

pub(crate) fn ownership_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        changing_window_ownership_prefers_real_buffers_and_uses_a_dummy_only_when_needed(),
        deleting_unowned_panes_preserves_both_purpose_and_buffer_owned_work(),
    ]
}
