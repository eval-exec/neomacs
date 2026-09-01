use expect_test::expect;

use super::ParityBatchCase;

fn installed_theme_tracks_a_real_file_and_its_mode_line_actions() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-spaceline-icons-test-with-theme-state
  (let* ((root (expand-file-name "spaceline-icons-file/" (getenv "HOME")))
         (file (expand-file-name "workspaces/release/controller.el" root))
         (buffer nil)
         (window (selected-window))
         (old-window-buffer (window-buffer window))
         (spaceline-all-the-icons-separator-type 'none)
         (spaceline-all-the-icons-clock-always-visible nil)
         (spaceline-all-the-icons-slim-render nil)
         (spaceline-all-the-icons-projectile-p nil)
         (spaceline-all-the-icons-hud-p nil)
         (spaceline-all-the-icons-time-p nil)
         (spaceline-all-the-icons-dedicated-p t)
         (spaceline-all-the-icons-narrowed-p t)
         (view-read-only nil))
    (unwind-protect
        (progn
          (make-directory (file-name-directory file) t)
          (with-temp-file file
            (insert "(defun deploy-release (service)\n"
                    "  (message \"deploy %s ✓\" service))\n"
                    "\n"
                    "(deploy-release 'checkout)\n"))
          (let ((enable-dir-local-variables nil))
            (setq buffer (find-file-noselect file)))
          (set-window-buffer window buffer)
          (with-current-buffer buffer
            (goto-char (point-max))
            (spaceline-all-the-icons-theme)
            (cl-letf (((symbol-function 'format-mode-line)
                       #'neomacs-spaceline-icons-test-format-mode-line))
              (let ((clean (spaceline-ml-all-the-icons)))
                (insert ";; pending production approval\n")
                (let* ((modified (spaceline-ml-all-the-icons))
                       (read-only-action
                        (neomacs-spaceline-icons-test-find-action
                         modified 'read-only-mode nil)))
                  (call-interactively read-only-action)
                  (let ((read-only (spaceline-ml-all-the-icons)))
                    (call-interactively read-only-action)
                    (save-buffer)
                    (goto-char (point-min))
                    (forward-line 1)
                    (let ((beginning (point)))
                      (forward-line 2)
                      (narrow-to-region beginning (point))
                      (let* ((narrowed (spaceline-ml-all-the-icons))
                             (widen-action
                              (neomacs-spaceline-icons-test-find-action
                               narrowed 'widen nil))
                             (was-narrowed (buffer-narrowed-p)))
                        (call-interactively widen-action)
                        (let* ((widened (spaceline-ml-all-the-icons))
                               (dedicate-action
                                (neomacs-spaceline-icons-test-find-action
                                 widened nil
                                 "Toggle `window-dedidcated' for this window")))
                          (call-interactively dedicate-action)
                          (let* ((dedicated (window-dedicated-p window))
                                 (dedicated-line (spaceline-ml-all-the-icons))
                                 (release-action
                                  (neomacs-spaceline-icons-test-find-action
                                   dedicated-line nil
                                   "Toggle `window-dedidcated' for this window")))
                            (call-interactively release-action)
                            (list
                             :installed (copy-tree (default-value 'mode-line-format))
                             :clean (neomacs-spaceline-icons-test-summary clean)
                             :modified
                             (list
                              (neomacs-spaceline-icons-test-visual-summary modified)
                              (neomacs-spaceline-icons-test-action-segment
                               modified 'read-only-mode nil))
                             :read-only
                             (list
                              (neomacs-spaceline-icons-test-visual-summary read-only)
                              (neomacs-spaceline-icons-test-action-segment
                               read-only 'read-only-mode nil))
                             :narrowed
                             (list
                              (neomacs-spaceline-icons-test-visual-summary narrowed)
                              (neomacs-spaceline-icons-test-action-segment
                               narrowed 'widen nil))
                             :widened
                             (neomacs-spaceline-icons-test-visual-summary widened)
                             :dedicated-line
                             (list
                              (neomacs-spaceline-icons-test-visual-summary dedicated-line)
                              (neomacs-spaceline-icons-test-action-segment
                               dedicated-line nil
                               "Toggle `window-dedidcated' for this window"))
                             :state
                             (list :read-only buffer-read-only
                                   :narrowed-before was-narrowed
                                   :narrowed-after (buffer-narrowed-p)
                                   :dedicated-during dedicated
                                   :dedicated-after
                                   (window-dedicated-p window)))))))))))))
      (when (window-live-p window)
        (set-window-dedicated-p window nil)
        (set-window-buffer window old-window-buffer))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (setq buffer-read-only nil)
          (set-buffer-modified-p nil))
        (kill-buffer buffer))
      (when (file-directory-p root)
        (delete-directory root t)))))
"####;
    let expected = expect![[
        r####"OK (:installed ("%e" (:eval (spaceline-ml-all-the-icons))) :clean (:text "   95   ~/spaceline-icons-file/workspaces/release/controller.el  5:0  " :codepoints (32 61633 32 61505 32 57 53 32 32 59686 32 126 47 115 112 97 99 101 108 105 110 101 45 105 99 111 110 115 45 102 105 108 101 47 119 111 114 107 115 112 97 99 101 115 47 114 101 108 101 97 115 101 47 99 111 110 116 114 111 108 108 101 114 46 101 108 32 32 53 58 48 32 32) :width 73 :faces ((0 1 spaceline-highlight-face) (1 2 (:family "FontAwesome" :height 1.1 :inherit spaceline-highlight-face)) (2 3 (spaceline-highlight-face)) (3 4 (:family "github-octicons" :height 1 :inherit spaceline-highlight-face)) (4 5 (spaceline-highlight-face)) (5 7 (:height 0.9 :inherit spaceline-highlight-face)) (7 8 spaceline-highlight-face) (8 9 powerline-active1) (9 10 (:height 1.1 :family "file-icons" :inherit powerline-active1)) (10 11 (powerline-active1)) (11 53 (:height 0.8 :inherit powerline-active1)) (53 66 (:height 0.8 :inherit powerline-active1)) (66 67 powerline-active1) (67 68 spaceline-highlight-face) (68 71 (:height 0.9 :inherit spaceline-highlight-face)) (71 72 spaceline-highlight-face) (72 73 powerline-active2)) :font-lock-faces ((1 2 (:family "FontAwesome" :height 1.2)) (3 4 (:family "github-octicons" :height 1.2)) (9 10 (:family "file-icons" :height 1.2 :inherit all-the-icons-purple))) :display ((1 2 (raise 0.0)) (3 4 (raise 0.1)) (5 7 (raise 0.1)) (9 10 (raise 0)) (11 53 (raise 0.2)) (53 66 (raise 0.2)) (68 71 (raise 0.1)) (72 73 ((space :align-to (- (+ right right-fringe right-margin) 0))))) :mouse-faces ((1 2 ((foreground-color . "#63B2FF"))) (3 4 ((foreground-color . "#63B2FF"))) (53 66 ((foreground-color . "#63B2FF")))) :help ((3 4 "Toggle `window-dedidcated' for this window") (9 10 "Major-mode: `emacs-lisp-mode'") (53 66 "Major-mode: `emacs-lisp-mode'")) :mouse-1 ((1 2 "" read-only-mode) (3 4 "" lambda) (53 66 "controller.el" find-file))) :modified ((:text "   126   ~/spaceline-icons-file/workspaces/release/controller.el  6:0  " :codepoints (32 61735 32 61505 32 49 50 54 32 32 59686 32 126 47 115 112 97 99 101 108 105 110 101 45 105 99 111 110 115 45 102 105 108 101 47 119 111 114 107 115 112 97 99 101 115 47 114 101 108 101 97 115 101 47 99 111 110 116 114 111 108 108 101 114 46 101 108 32 32 54 58 48 32 32) :width 74) (:range (1 2) :text "" :codepoints (61735) :face (:family "FontAwesome" :height 1.1 :inherit spaceline-highlight-face) :font-lock-face (:family "FontAwesome" :height 1.2) :display (raise 0.0) :mouse-face ((foreground-color . "#63B2FF")) :help nil :mouse-1 read-only-mode)) :read-only ((:text "   126   ~/spaceline-icons-file/workspaces/release/controller.el  6:0  " :codepoints (32 61475 32 61505 32 49 50 54 32 32 59686 32 126 47 115 112 97 99 101 108 105 110 101 45 105 99 111 110 115 45 102 105 108 101 47 119 111 114 107 115 112 97 99 101 115 47 114 101 108 101 97 115 101 47 99 111 110 116 114 111 108 108 101 114 46 101 108 32 32 54 58 48 32 32) :width 74) (:range (1 2) :text "" :codepoints (61475) :face (:family "FontAwesome" :height 1.1 :inherit spaceline-highlight-face) :font-lock-face (:family "FontAwesome" :height 1.2) :display (raise 0.0) :mouse-face ((foreground-color . "#63B2FF")) :help nil :mouse-1 read-only-mode)) :narrowed ((:text "   126   ~/spaceline-icons-file/workspaces/release/controller.el  3:0 |   " :codepoints (32 61633 32 61505 32 49 50 54 32 32 59686 32 126 47 115 112 97 99 101 108 105 110 101 45 105 99 111 110 115 45 102 105 108 101 47 119 111 114 107 115 112 97 99 101 115 47 114 101 108 101 97 115 101 47 99 111 110 116 114 111 108 108 101 114 46 101 108 32 32 51 58 48 32 124 32 61616 32 32) :width 78) (:range (75 76) :text "" :codepoints (61616) :face (:height 0.9 :inherit spaceline-highlight-face) :font-lock-face (:family "FontAwesome" :height 1.2) :display (raise 0.12) :mouse-face ((foreground-color . "#63B2FF")) :help "mouse-1: Widen the current file" :mouse-1 widen)) :widened (:text "   126   ~/spaceline-icons-file/workspaces/release/controller.el  4:0  " :codepoints (32 61633 32 61505 32 49 50 54 32 32 59686 32 126 47 115 112 97 99 101 108 105 110 101 45 105 99 111 110 115 45 102 105 108 101 47 119 111 114 107 115 112 97 99 101 115 47 114 101 108 101 97 115 101 47 99 111 110 116 114 111 108 108 101 114 46 101 108 32 32 52 58 48 32 32) :width 74) :dedicated-line ((:text "   126   ~/spaceline-icons-file/workspaces/release/controller.el  4:0  " :codepoints (32 61633 32 61581 32 49 50 54 32 32 59686 32 126 47 115 112 97 99 101 108 105 110 101 45 105 99 111 110 115 45 102 105 108 101 47 119 111 114 107 115 112 97 99 101 115 47 114 101 108 101 97 115 101 47 99 111 110 116 114 111 108 108 101 114 46 101 108 32 32 52 58 48 32 32) :width 74) (:range (3 4) :text "" :codepoints (61581) :face (:family "FontAwesome" :height 1 :inherit spaceline-highlight-face) :font-lock-face (:family "FontAwesome" :height 1.2) :display (raise 0.1) :mouse-face ((foreground-color . "#63B2FF")) :help "Toggle `window-dedidcated' for this window" :mouse-1 lambda)) :state (:read-only nil :narrowed-before t :narrowed-after nil :dedicated-during t :dedicated-after nil))"####
    ]];
    ParityBatchCase::value(
        "installed_theme_tracks_a_real_file_and_its_mode_line_actions",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![installed_theme_tracks_a_real_file_and_its_mode_line_actions()]
}
