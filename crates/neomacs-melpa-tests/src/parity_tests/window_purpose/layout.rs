use expect_test::expect;

use super::ParityBatchCase;

fn a_project_layout_round_trip_restores_purpose_topology_and_recent_history() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-window-purpose-test-with-configuration
    nil
    nil
    '(("wp-layout-main.el" . edit)
      ("wp-layout-tests.el" . edit)
      ("*wp-layout-repl*" . terminal)
      ("wp-layout-mutated" . general))
    nil
  (let ((main (get-buffer-create "wp-layout-main.el"))
        (tests (get-buffer-create "wp-layout-tests.el"))
        (repl (get-buffer-create "*wp-layout-repl*"))
        (mutated (get-buffer-create "wp-layout-mutated"))
        (purpose-recent-window-layouts (make-ring 4))
        result)
    (save-window-excursion
      (unwind-protect
          (progn
            (delete-other-windows)
            (let* ((main-window (selected-window))
                   (repl-window (split-window main-window 28 'right))
                   (tests-window (split-window main-window 6 'below)))
              (set-window-buffer main-window main)
              (set-window-buffer tests-window tests)
              (set-window-buffer repl-window repl)
              (purpose-set-window-purpose-dedicated-p repl-window t)
              (set-window-dedicated-p tests-window t)
              (let* ((layout (purpose-get-window-layout))
                     (saved-contract
                      (neomacs-window-purpose-test-layout-contract layout))
                     (before
                      (neomacs-window-purpose-test-window-snapshot)))
                (delete-other-windows main-window)
                (set-window-buffer nil mutated)
                (purpose-set-window-layout layout)
                (let ((restored
                       (neomacs-window-purpose-test-window-snapshot))
                      (restored-contract
                       (neomacs-window-purpose-test-layout-contract
                        (purpose-get-window-layout))))
                  (delete-other-windows)
                  (set-window-buffer nil mutated)
                  (purpose-reset-window-layout)
                  (setq result
                        (list
                         :saved-contract saved-contract
                         :before before
                         :restored-contract restored-contract
                         :restored restored
                         :recent-count
                         (ring-length purpose-recent-window-layouts)
                         :recent-is-saved
                         (equal (ring-ref purpose-recent-window-layouts 0)
                                layout)
                         :after-reset-contract
                         (neomacs-window-purpose-test-layout-contract
                          (purpose-get-window-layout))
                         :after-reset
                         (neomacs-window-purpose-test-window-snapshot)
                         :recent-count-after-reset
                         (ring-length purpose-recent-window-layouts))))))
            result)
        (dolist (window (window-list nil 'nomini))
          (set-window-dedicated-p window nil)
          (purpose-set-window-purpose-dedicated-p window nil))
        (neomacs-window-purpose-test-kill-buffers
         main tests repl mutated)))))
"##;
    let expect = expect![[
        r#"OK (:saved-contract (:split left-right :children ((:split top-bottom :children ((:purpose edit :purpose-dedicated nil) (:purpose edit :purpose-dedicated nil))) (:purpose terminal :purpose-dedicated t))) :before (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-layout-main.el" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top (left 28) (top 6))) (:buffer "*wp-layout-repl*" :purpose terminal :purpose-dedicated t :buffer-dedicated nil :selected nil :edges ((left 28) top right bottom)) (:buffer "wp-layout-tests.el" :purpose edit :purpose-dedicated nil :buffer-dedicated t :selected nil :edges (left (top 6) (left 28) bottom)))) :restored-contract (:split left-right :children ((:split top-bottom :children ((:purpose edit :purpose-dedicated nil) (:purpose edit :purpose-dedicated nil))) (:purpose terminal :purpose-dedicated t))) :restored (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-layout-main.el" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top (left 28) (top 6))) (:buffer "*wp-layout-repl*" :purpose terminal :purpose-dedicated t :buffer-dedicated nil :selected nil :edges ((left 28) top right bottom)) (:buffer "wp-layout-tests.el" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges (left (top 6) (left 28) bottom)))) :recent-count 1 :recent-is-saved t :after-reset-contract (:split left-right :children ((:split top-bottom :children ((:purpose edit :purpose-dedicated nil) (:purpose edit :purpose-dedicated nil))) (:purpose terminal :purpose-dedicated t))) :after-reset (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-layout-main.el" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top (left 28) (top 6))) (:buffer "*wp-layout-repl*" :purpose terminal :purpose-dedicated t :buffer-dedicated nil :selected nil :edges ((left 28) top right bottom)) (:buffer "wp-layout-tests.el" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges (left (top 6) (left 28) bottom)))) :recent-count-after-reset 1)"#
    ]];
    ParityBatchCase::value(
        "a_project_layout_round_trip_restores_purpose_topology_and_recent_history",
        elisp_form,
        expect,
    )
}

fn named_layout_files_honor_directory_priority_and_reject_truncation_atomically() -> ParityBatchCase
{
    let elisp_form = r##"
(neomacs-window-purpose-test-with-configuration
    nil
    nil
    '(("wp-persist-main.el" . edit)
      ("*wp-persist-repl*" . terminal)
      ("wp-persist-guide" . docs)
      ("wp-persist-search" . search))
    nil
  (let* ((root (expand-file-name "window-purpose-layouts/"
                                 (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
         (high (expand-file-name "project/" root))
         (low (expand-file-name "fallback/" root))
         (broken (expand-file-name "broken.window-layout" high))
         (main (get-buffer-create "wp-persist-main.el"))
         (repl (get-buffer-create "*wp-persist-repl*"))
         (guide (get-buffer-create "wp-persist-guide"))
         (search (get-buffer-create "wp-persist-search"))
         (purpose-layout-dirs (list high low))
         (purpose-use-built-in-layouts nil)
         (purpose-recent-window-layouts (make-ring 4))
         result)
    (save-window-excursion
      (unwind-protect
          (progn
            (make-directory high t)
            (make-directory low t)
            (delete-other-windows)
            (let ((right (split-window nil 24 'right)))
              (set-window-buffer nil main)
              (set-window-buffer right repl)
              (purpose-set-window-purpose-dedicated-p right t)
              (purpose-save-window-layout "workspace" high))
            (delete-other-windows)
            (let ((bottom (split-window nil 7 'below)))
              (set-window-buffer nil guide)
              (set-window-buffer bottom search)
              (purpose-save-window-layout "workspace" low))
            (let* ((catalog (purpose-all-window-layouts nil nil))
                   (found (purpose-find-window-layout "workspace"))
                   (found-in-high
                    (equal (file-name-directory found)
                           (file-name-as-directory high))))
              (purpose-load-window-layout "workspace")
              (let ((loaded-contract
                     (neomacs-window-purpose-test-layout-contract
                      (purpose-get-window-layout)))
                    (loaded
                     (neomacs-window-purpose-test-window-snapshot)))
                (with-temp-file broken
                  (insert "("))
                (let* ((before-broken (purpose-get-window-layout))
                       (broken-signal
                        (condition-case error-data
                            (progn
                              (purpose-load-window-layout-file broken)
                              'no-signal)
                          (error (car error-data))))
                       (after-broken (purpose-get-window-layout)))
                  (setq result
                        (list
                         :saved-files
                         (list
                          (file-exists-p
                           (expand-file-name "workspace.window-layout"
                                             high))
                          (file-exists-p
                           (expand-file-name "workspace.window-layout"
                                             low)))
                         :catalog catalog
                         :found-in-high found-in-high
                         :loaded-contract loaded-contract
                         :loaded loaded
                         :recent-count
                         (ring-length purpose-recent-window-layouts)
                         :broken-signal broken-signal
                         :broken-atomic
                         (equal before-broken after-broken))))))
            result)
        (dolist (window (window-list nil 'nomini))
          (set-window-dedicated-p window nil)
          (purpose-set-window-purpose-dedicated-p window nil))
        (neomacs-window-purpose-test-kill-buffers
         main repl guide search)))))
"##;
    let expect = expect![[
        r#"OK (:saved-files (t t) :catalog ("workspace") :found-in-high t :loaded-contract (:split left-right :children ((:purpose edit :purpose-dedicated nil) (:purpose terminal :purpose-dedicated t))) :loaded (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-persist-main.el" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top (left 24) bottom)) (:buffer "*wp-persist-repl*" :purpose terminal :purpose-dedicated t :buffer-dedicated nil :selected nil :edges ((left 24) top right bottom)))) :recent-count 1 :broken-signal end-of-file :broken-atomic t)"#
    ]];
    ParityBatchCase::value(
        "named_layout_files_honor_directory_priority_and_reject_truncation_atomically",
        elisp_form,
        expect,
    )
}

pub(crate) fn layout_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        a_project_layout_round_trip_restores_purpose_topology_and_recent_history(),
        named_layout_files_honor_directory_priority_and_reject_truncation_atomically(),
    ]
}
