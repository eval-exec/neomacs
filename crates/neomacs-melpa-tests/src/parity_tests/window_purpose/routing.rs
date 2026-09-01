use expect_test::expect;

use super::ParityBatchCase;

fn public_switch_key_routes_normal_and_forced_requests_but_bypasses_with_c_u() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-window-purpose-test-with-configuration
    nil
    nil nil
    '(("^wp-edit-" . edit)
      ("^wp-repl-" . terminal)
      ("^wp-docs" . docs))
  (let ((main (get-buffer-create "wp-edit-main"))
        (tests (get-buffer-create "wp-edit-tests"))
        (repl (get-buffer-create "wp-repl-python"))
        (docs (get-buffer-create "wp-docs"))
        (docs-guide (get-buffer-create "wp-docs-guide"))
        (split-width-threshold 1)
        (split-height-threshold nil)
        (pop-up-windows t)
        (pop-up-frames nil)
        (purpose-display-fallback 'pop-up-window)
        result)
    (save-window-excursion
      (unwind-protect
          (progn
            (delete-other-windows)
            (let* ((edit-window (selected-window))
                   (terminal-window (split-window edit-window nil 'right)))
              (set-window-buffer edit-window main)
              (set-window-buffer terminal-window repl)
              (purpose-set-window-purpose-dedicated-p edit-window t)
              (purpose-set-window-purpose-dedicated-p terminal-window t)
              (select-window terminal-window)
              (purpose-mode 1)
              (let ((initial
                     (neomacs-window-purpose-test-window-snapshot))
                    (switch-command (key-binding (kbd "C-x b"))))
                ;; The public unprefixed `C-x b' keeps both purpose-owned panes
                ;; and routes TESTS through Purpose's documented fallback.
                (cl-letf (((symbol-function 'read-buffer-to-switch)
                           (lambda (&rest _arguments)
                             (buffer-name tests))))
                  (let ((current-prefix-arg nil))
                    (call-interactively switch-command)))
                (let ((routed
                       (neomacs-window-purpose-test-window-snapshot)))
                  ;; Strong GNU buffer dedication is independent from Purpose
                  ;; dedication.  MAIN is already displayed in that owned pane;
                  ;; the public switch still exercises fallback ownership.
                  (set-window-dedicated-p edit-window t)
                  (select-window terminal-window)
                  (cl-letf (((symbol-function 'read-buffer-to-switch)
                             (lambda (&rest _arguments)
                               (buffer-name main))))
                    (let ((current-prefix-arg nil))
                      (call-interactively switch-command)))
                  (let ((buffer-dedicated-fallback
                         (neomacs-window-purpose-test-window-snapshot)))
                    ;; The advertised `C-u C-x b' escape hatch deliberately
                    ;; bypasses Purpose.  It changes the terminal-owned pane to
                    ;; DOCS while leaving purpose dedication intact.
                    (select-window terminal-window)
                    (cl-letf (((symbol-function 'read-buffer-to-switch)
                               (lambda (&rest _arguments)
                                 (buffer-name docs))))
                      (let ((current-prefix-arg '(4)))
                        (call-interactively switch-command)))
                    (let ((after-explicit-bypass
                           (neomacs-window-purpose-test-window-snapshot))
                          (bypassed-modeline
                           (purpose--modeline-string)))
                      ;; `C-u C-u C-x b' takes the third public branch: choose
                      ;; another buffer with the current buffer's DOCS purpose
                      ;; and route it through Purpose again.
                      (cl-letf
                          (((symbol-function 'completing-read)
                            (lambda (&rest _arguments)
                              (buffer-name docs-guide))))
                        (let ((current-prefix-arg '(16)))
                          (call-interactively switch-command)))
                      (setq result
                            (list
                             :key-binding switch-command
                             :prefix-indices
                             (mapcar #'purpose--prefix-arg-to-index
                                     '(nil (4) (16)))
                             :initial initial
                             :routed routed
                             :buffer-dedicated-fallback
                             buffer-dedicated-fallback
                             :after-explicit-bypass after-explicit-bypass
                             :bypassed-modeline bypassed-modeline
                             :after-forced-purpose
                             (neomacs-window-purpose-test-window-snapshot)
                             :forced-buffer
                             (buffer-name (window-buffer)))))))))
            result)
        (when purpose-mode
          (purpose-mode -1))
        (dolist (window (window-list nil 'nomini))
          (set-window-dedicated-p window nil)
          (purpose-set-window-purpose-dedicated-p window nil))
        (neomacs-window-purpose-test-kill-buffers
         main tests repl docs docs-guide)))))
"##;
    let expect = expect![[
        r#"OK (:key-binding purpose-switch-buffer-overload :prefix-indices (0 1 2) :initial (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-edit-main" :purpose edit :purpose-dedicated t :buffer-dedicated nil :selected nil :edges (left top (left 40) bottom)) (:buffer "wp-repl-python" :purpose terminal :purpose-dedicated t :buffer-dedicated nil :selected t :edges ((left 40) top right bottom)))) :routed (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-edit-main" :purpose edit :purpose-dedicated t :buffer-dedicated nil :selected nil :edges (left top (left 40) bottom)) (:buffer "wp-repl-python" :purpose terminal :purpose-dedicated t :buffer-dedicated nil :selected nil :edges ((left 40) top (right 20) bottom)) (:buffer "wp-edit-tests" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges ((right 20) top right bottom)))) :buffer-dedicated-fallback (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-edit-main" :purpose edit :purpose-dedicated t :buffer-dedicated t :selected nil :edges (left top (left 20) bottom)) (:buffer "wp-edit-main" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected t :edges ((left 20) top (left 40) bottom)) (:buffer "wp-repl-python" :purpose terminal :purpose-dedicated t :buffer-dedicated nil :selected nil :edges ((left 40) top (right 20) bottom)) (:buffer "wp-edit-tests" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges ((right 20) top right bottom)))) :after-explicit-bypass (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-edit-main" :purpose edit :purpose-dedicated t :buffer-dedicated t :selected nil :edges (left top (left 20) bottom)) (:buffer "wp-edit-main" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges ((left 20) top (left 40) bottom)) (:buffer "wp-docs" :purpose docs :purpose-dedicated t :buffer-dedicated nil :selected t :edges ((left 40) top (right 20) bottom)) (:buffer "wp-edit-tests" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges ((right 20) top right bottom)))) :bypassed-modeline " [docs!]" :after-forced-purpose (:frame-size (80 25) :root-horizontal-span (0 80) :root-vertical-span (1 24) :windows ((:buffer "wp-edit-main" :purpose edit :purpose-dedicated t :buffer-dedicated t :selected nil :edges (left top (left 20) bottom)) (:buffer "wp-edit-main" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges ((left 20) top (left 40) bottom)) (:buffer "wp-docs-guide" :purpose docs :purpose-dedicated t :buffer-dedicated nil :selected t :edges ((left 40) top (right 20) bottom)) (:buffer "wp-edit-tests" :purpose edit :purpose-dedicated nil :buffer-dedicated nil :selected nil :edges ((right 20) top right bottom)))) :forced-buffer "wp-docs-guide")"#
    ]];
    ParityBatchCase::value(
        "public_switch_key_routes_normal_and_forced_requests_but_bypasses_with_c_u",
        elisp_form,
        expect,
    )
}

pub(crate) fn routing_batch_cases() -> Vec<ParityBatchCase> {
    vec![public_switch_key_routes_normal_and_forced_requests_but_bypasses_with_c_u()]
}
