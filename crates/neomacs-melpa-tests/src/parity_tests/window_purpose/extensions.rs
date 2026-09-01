use expect_test::expect;

use super::ParityBatchCase;

fn killing_work_buffers_reuses_same_purpose_then_removes_an_empty_owned_pane() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (require 'window-purpose-x)
  (neomacs-window-purpose-test-with-configuration
      nil
      nil
      '(("wp-kill-source-a.el" . edit)
        ("wp-kill-source-b.el" . edit)
        ("*wp-kill-repl*" . terminal))
      nil
    (let ((source-a (get-buffer-create "wp-kill-source-a.el"))
          (source-b (get-buffer-create "wp-kill-source-b.el"))
          (repl (get-buffer-create "*wp-kill-repl*"))
          result)
      (save-window-excursion
        (unwind-protect
            (progn
              (delete-other-windows)
              (let* ((edit-window (selected-window))
                     (repl-window (split-window edit-window 28 'right)))
                (set-window-buffer edit-window source-a)
                (set-window-buffer repl-window repl)
                (purpose-set-window-purpose-dedicated-p edit-window t)
                (purpose-mode 1)
                (purpose-x-kill-setup)
                (let ((setup
                       (list
                        :advice
                        (and
                         (advice-member-p
                          #'purpose-x-replace-buffer-in-windows
                          'replace-buffer-in-windows)
                         t)
                        :hook
                        (and (memq 'purpose-x-kill-sync purpose-mode-hook)
                             t)
                        :windows
                        (neomacs-window-purpose-test-window-snapshot))))
                  (kill-buffer source-a)
                  (let ((after-first
                         (list
                          :source-a-live (buffer-live-p source-a)
                          :replacement
                          (buffer-name (window-buffer edit-window))
                          :purpose
                          (purpose-window-purpose edit-window)
                          :purpose-dedicated
                          (and
                           (purpose-window-purpose-dedicated-p edit-window)
                           t)
                          :window-count
                          (length (window-list nil 'nomini)))))
                    (kill-buffer source-b)
                    (let ((after-second
                           (list
                            :source-b-live (buffer-live-p source-b)
                            :edit-window-live (window-live-p edit-window)
                            :windows
                            (neomacs-window-purpose-test-window-snapshot))))
                      (purpose-x-kill-unset)
                      (setq result
                            (list
                             :setup setup
                             :after-first after-first
                             :after-second after-second
                             :after-unset
                             (list
                              :advice
                              (and
                               (advice-member-p
                                #'purpose-x-replace-buffer-in-windows
                                'replace-buffer-in-windows)
                               t)
                              :hook
                              (and
                               (memq 'purpose-x-kill-sync purpose-mode-hook)
                               t))))))))
              result)
          (purpose-x-kill-unset)
          (when purpose-mode
            (purpose-mode -1))
          (dolist (window (window-list nil 'nomini))
            (set-window-dedicated-p window nil)
            (purpose-set-window-purpose-dedicated-p window nil))
          (neomacs-window-purpose-test-kill-buffers
           source-a source-b repl))))))
"##;
    let expect = expect![[
        r#"OK (:setup (:advice t :hook t :windows (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-kill-source-a.el" :purpose edit :purpose-dedicated t :buffer-dedicated nil :selected t :edges (left top (left 28) bottom)) (:buffer "*wp-kill-repl*" :purpose terminal :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges ((left 28) top right bottom))))) :after-first (:source-a-live nil :replacement "wp-kill-source-b.el" :purpose edit :purpose-dedicated t :window-count 2) :after-second (:source-b-live nil :edit-window-live nil :windows (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "*wp-kill-repl*" :purpose terminal :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top right bottom))))) :after-unset (:advice nil :hook nil))"#
    ]];
    ParityBatchCase::value(
        "killing_work_buffers_reuses_same_purpose_then_removes_an_empty_owned_pane",
        elisp_form,
        expect,
    )
}

pub(crate) fn extension_batch_cases() -> Vec<ParityBatchCase> {
    vec![killing_work_buffers_reuses_same_purpose_then_removes_an_empty_owned_pane()]
}
