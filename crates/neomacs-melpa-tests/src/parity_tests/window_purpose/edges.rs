use expect_test::expect;

use super::ParityBatchCase;

fn edge_panes_reuse_by_purpose_respect_buffer_ownership_and_reject_bad_sizes_atomically()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-window-purpose-test-with-configuration
    nil
    nil nil
    '(("^wp-editor$" . edit)
      ("^wp-search-" . search)
      ("^wp-docs-" . docs)
      ("^wp-help$" . help))
  (let ((editor (get-buffer-create "wp-editor"))
        (search-one (get-buffer-create "wp-search-rust"))
        (search-two (get-buffer-create "wp-search-elisp"))
        (docs-one (get-buffer-create "wp-docs-api"))
        (docs-two (get-buffer-create "wp-docs-guide"))
        (help (get-buffer-create "wp-help"))
        result)
    (save-window-excursion
      (unwind-protect
          (progn
            (delete-other-windows)
            (set-window-buffer nil editor)
            (let* ((bottom-one
                    (purpose-display-at-bottom search-one nil 4))
                   (bottom-created
                    (and (eq bottom-one (purpose-get-bottom-window))
                         (buffer-name (window-buffer bottom-one))))
                   (after-bottom-one
                    (neomacs-window-purpose-test-window-snapshot))
                   (bottom-two
                    (purpose-display-at-bottom search-two nil 4))
                   (after-bottom-two
                    (neomacs-window-purpose-test-window-snapshot))
                   (right-one
                    (purpose-display-at-right docs-one nil 12))
                   (right-created
                    (and (eq right-one (purpose-get-right-window))
                         (buffer-name (window-buffer right-one))))
                   (before-invalid
                    (neomacs-window-purpose-test-window-snapshot))
                   (invalid
                    (condition-case error-data
                        (progn
                          (purpose-display-at-right help nil 0)
                          :no-error)
                      (error
                       (list (car error-data) (cdr error-data)))))
                   (after-invalid
                    (neomacs-window-purpose-test-window-snapshot)))
              (set-window-dedicated-p right-one t)
              (let* ((right-two
                      (purpose-display-at-right docs-two nil 10))
                     (second-right-created
                      (and (window-live-p right-two)
                           (not (eq right-one right-two))
                           (buffer-name (window-buffer right-two))))
                     (after-buffer-dedicated
                      (neomacs-window-purpose-test-window-snapshot)))
                (purpose-delete-window-at-right)
                (setq result
                      (list
                       :bottom-created
                       bottom-created
                       :after-bottom-one after-bottom-one
                       :bottom-reused (eq bottom-one bottom-two)
                       :after-bottom-two after-bottom-two
                       :right-created
                       right-created
                       :invalid invalid
                       :invalid-atomic
                       (equal before-invalid after-invalid)
                       :before-invalid before-invalid
                       :second-right-created
                       second-right-created
                       :after-buffer-dedicated
                       after-buffer-dedicated
                       :deleted-second-right
                       (not (window-live-p right-two))
                       :revealed-right
                       (and (purpose-get-right-window)
                            (buffer-name
                             (window-buffer
                              (purpose-get-right-window))))
                       :after-delete
                       (neomacs-window-purpose-test-window-snapshot))))
            result))
        (dolist (window (window-list nil 'nomini))
          (set-window-dedicated-p window nil)
          (purpose-set-window-purpose-dedicated-p window nil))
        (neomacs-window-purpose-test-kill-buffers
         editor search-one search-two docs-one docs-two help)))))
"##;
    let expect = expect![[
        r#"OK (:bottom-created "wp-search-rust" :after-bottom-one (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-editor" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top right (bottom 4))) (:buffer "wp-search-rust" :purpose search :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges (left (bottom 4) right bottom)))) :bottom-reused t :after-bottom-two (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-editor" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top right (bottom 4))) (:buffer "wp-search-elisp" :purpose search :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges (left (bottom 4) right bottom)))) :right-created "wp-docs-api" :invalid (wrong-type-argument ("positive integer or percentage" 0)) :invalid-atomic t :before-invalid (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-editor" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top (right 12) (bottom 4))) (:buffer "wp-docs-api" :purpose docs :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges ((right 12) top right bottom)) (:buffer "wp-search-elisp" :purpose search :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges (left (bottom 4) (right 12) bottom)))) :second-right-created "wp-docs-guide" :after-buffer-dedicated (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-editor" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top (right 20) (bottom 4))) (:buffer "wp-docs-api" :purpose docs :purpose-dedicated nil :buffer-dedicated t :selected nil :edges ((right 20) top (right 10) bottom)) (:buffer "wp-docs-guide" :purpose docs :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges ((right 10) top right bottom)) (:buffer "wp-search-elisp" :purpose search :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges (left (bottom 4) (right 20) bottom)))) :deleted-second-right t :revealed-right "wp-docs-api" :after-delete (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-editor" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges (left top (right 12) (bottom 4))) (:buffer "wp-docs-api" :purpose docs :purpose-dedicated nil :buffer-dedicated t :selected nil :edges ((right 12) top right bottom)) (:buffer "wp-search-elisp" :purpose search :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges (left (bottom 4) (right 12) bottom)))))"#
    ]];
    ParityBatchCase::value(
        "edge_panes_reuse_by_purpose_respect_buffer_ownership_and_reject_bad_sizes_atomically",
        elisp_form,
        expect,
    )
}

pub(crate) fn edge_batch_cases() -> Vec<ParityBatchCase> {
    vec![edge_panes_reuse_by_purpose_respect_buffer_ownership_and_reject_bad_sizes_atomically()]
}
