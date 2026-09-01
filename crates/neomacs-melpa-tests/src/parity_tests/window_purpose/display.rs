use expect_test::expect;

use super::ParityBatchCase;

fn search_results_follow_user_and_special_actions_without_leaking_failed_displays()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-window-purpose-test-with-configuration
    nil
    nil nil
    '(("^wp-search-" . search)
      ("^wp-editor$" . edit))
  (let ((editor (get-buffer-create "wp-editor"))
        (first-search (get-buffer-create "wp-search-rust"))
        (second-search (get-buffer-create "wp-search-elisp"))
        (ignored-search (get-buffer-create "wp-search-ignored"))
        (failed-search (get-buffer-create "wp-search-failed"))
        (purpose-special-action-sequences
         '((neomacs-window-purpose-test-search-p
            neomacs-window-purpose-test-display-search-at-bottom)))
        (purpose-display-buffer-functions
         '(neomacs-window-purpose-test-after-display))
        (purpose-display-fallback 'error)
        (purpose-action-function-ignore-buffer-names
         '("^wp-search-ignored$"))
        (neomacs-window-purpose-test-display-trace nil)
        result)
    (save-window-excursion
      (unwind-protect
          (progn
            (delete-other-windows)
            (set-window-buffer nil editor)
            (purpose-mode 1)
            (let* ((selected-before (selected-window))
                   (first-window
                    (display-buffer
                     first-search
                     '(neomacs-window-purpose-test-decline-display)))
                   (first-buffer-at-return
                    (and (window-live-p first-window)
                         (buffer-name (window-buffer first-window))))
                   (after-first
                    (neomacs-window-purpose-test-window-snapshot))
                   (second-window (display-buffer second-search))
                   (after-second
                    (neomacs-window-purpose-test-window-snapshot))
                   (ignored-result
                    (display-buffer
                     ignored-search
                     '((display-buffer-no-window)
                       (allow-no-window . t))))
                   (after-ignored
                    (neomacs-window-purpose-test-window-snapshot))
                   (failed-result
                    (display-buffer
                     failed-search
                     '(neomacs-window-purpose-test-return-fail)))
                   (after-failed
                    (neomacs-window-purpose-test-window-snapshot)))
              (setq result
                    (list
                     :first-returned
                     first-buffer-at-return
                     :selected-preserved
                     (eq selected-before (selected-window))
                     :after-first after-first
                     :second-reused-first (eq first-window second-window)
                     :after-second after-second
                     :ignored-result ignored-result
                     :ignored-buffer-visible
                     (and (get-buffer-window ignored-search) t)
                     :after-ignored after-ignored
                     :failed-result failed-result
                     :failed-buffer-visible
                     (and (get-buffer-window failed-search) t)
                     :after-failed after-failed
                     :trace
                     (nreverse
                      neomacs-window-purpose-test-display-trace))))
            result)
        (when purpose-mode
          (purpose-mode -1))
        (dolist (window (window-list nil 'nomini))
          (set-window-dedicated-p window nil)
          (purpose-set-window-purpose-dedicated-p window nil))
        (neomacs-window-purpose-test-kill-buffers
         editor first-search second-search ignored-search failed-search)))))
"##;
    let expect = expect![[
        r#"OK (:first-returned "wp-search-rust" :selected-preserved t :after-first (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-editor" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top right (bottom 5))) (:buffer "wp-search-rust" :purpose search :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges (left (bottom 5) right bottom)))) :second-reused-first t :after-second (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-editor" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top right (bottom 5))) (:buffer "wp-search-elisp" :purpose search :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges (left (bottom 5) right bottom)))) :ignored-result nil :ignored-buffer-visible nil :after-ignored (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-editor" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top right (bottom 5))) (:buffer "wp-search-elisp" :purpose search :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges (left (bottom 5) right bottom)))) :failed-result nil :failed-buffer-visible nil :after-failed (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-editor" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top right (bottom 5))) (:buffer "wp-search-elisp" :purpose search :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges (left (bottom 5) right bottom)))) :trace ((:predicate search "wp-search-rust") (:user-declined "wp-search-rust") (:special "wp-search-rust") (:hook "wp-search-rust" nil) (:predicate search "wp-search-elisp") (:special "wp-search-elisp") (:hook "wp-search-elisp" nil) (:predicate search "wp-search-failed") (:user-failed "wp-search-failed") (:user-failed "wp-search-failed")))"#
    ]];
    ParityBatchCase::value(
        "search_results_follow_user_and_special_actions_without_leaking_failed_displays",
        elisp_form,
        expect,
    )
}

pub(crate) fn display_batch_cases() -> Vec<ParityBatchCase> {
    vec![search_results_follow_user_and_special_actions_without_leaking_failed_displays()]
}
